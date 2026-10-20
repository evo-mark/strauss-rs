//! Strauss configuration, read from `strauss.json` in the project root.
//!
//! PHP Strauss reads the same options from `extra.strauss` in `composer.json`.
//! The option names, shapes, defaults and inference rules here follow
//! `StraussConfig.php` so that an existing `extra.strauss` object can be moved
//! into `strauss.json` unchanged. `composer.json` is still read, but only to
//! infer values that were not set (prefixes and the package list), as PHP
//! Strauss does.
//!
//! Where this deliberately differs from PHP Strauss:
//!
//! * `strauss.json` is the options object itself, with no `extra.strauss`
//!   wrapper.
//! * Unknown keys are an error, so a typo can't silently do nothing.
//! * Values are validated when loaded (prefixes must be usable PHP names,
//!   regular expressions must compile, `target_directory` must stay inside the
//!   project) rather than failing, or misbehaving, later.
//! * A `classmap_prefix` inferred from the package name ends in an underscore
//!   (`Acme_My_Plugin_`). PHP Strauss leaves it off, which runs the prefix into
//!   the class name.
//! * Mozart's legacy keys (`dep_directory`, `dep_namespace`,
//!   `exclude_packages`, …) and the `extra.mozart` fallback are not supported.
#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::Path;

use crate::config::composer::ComposerJson;
use crate::config::error::invalid;
use crate::config::infer::{autoloads_directory, infer_classmap_prefix, infer_namespace_prefix};
use crate::config::raw::{RawConfig, RawExclusions};
use crate::config::validate::{
    check_shape, is_identifier, normalise_target_directory, require_non_empty, resolve_exclusions,
    resolve_functions_prefix, resolve_parallel, resolve_prefix_exclusions, resolve_presets,
    resolve_update_call_sites, validate_name_prefix, validate_namespace_prefix,
};
use crate::php_pattern::{PhpPattern, Replacement};

mod composer;
mod error;
mod infer;
mod model;
mod raw;
mod validate;

// The option types are this module's API; nothing outside it names them yet.
#[allow(unused_imports)]
pub use composer::{Autoload, Ordered, StringOrList};
#[allow(unused_imports)]
pub use error::ConfigError;
#[allow(unused_imports)]
pub use model::*;

pub const CONFIG_FILE: &str = "strauss.json";
pub const COMPOSER_FILE: &str = "composer.json";

const DEFAULT_TARGET_DIRECTORY: &str = "vendor-prefixed";
const DEFAULT_VENDOR_DIRECTORY: &str = "vendor";

impl Config {
    /// Reads `strauss.json` and `composer.json` from `project_dir`. Neither
    /// has to exist: Strauss needs no configuration to run.
    pub fn load(project_dir: &Path) -> Result<Config, ConfigError> {
        let strauss = read_optional(&project_dir.join(CONFIG_FILE))?;
        let composer = read_optional(&project_dir.join(COMPOSER_FILE))?;

        Config::from_json(project_dir, strauss.as_deref(), composer.as_deref())
    }

    /// Builds the configuration from the contents of the two files.
    pub fn from_json(
        project_dir: &Path,
        strauss_json: Option<&str>,
        composer_json: Option<&str>,
    ) -> Result<Config, ConfigError> {
        if let Some(json) = strauss_json {
            check_shape(json)?;
        }

        let raw: RawConfig = match strauss_json {
            Some(json) => serde_json::from_str(json).map_err(|error| ConfigError::Parse {
                file: CONFIG_FILE,
                message: error.to_string(),
            })?,
            None => RawConfig::default(),
        };

        let composer: ComposerJson = match composer_json {
            Some(json) => serde_json::from_str(json).map_err(|error| ConfigError::Parse {
                file: COMPOSER_FILE,
                message: error.to_string(),
            })?,
            None => ComposerJson::default(),
        };

        let target_directory = match &raw.target_directory {
            Some(directory) => normalise_target_directory(directory)?,
            None => DEFAULT_TARGET_DIRECTORY.to_string(),
        };

        let explicit_namespace_prefix = raw
            .namespace_prefix
            .as_deref()
            .map(|prefix| validate_namespace_prefix("namespace_prefix", prefix))
            .transpose()?;

        let explicit_classmap_prefix = raw
            .classmap_prefix
            .as_deref()
            .map(|prefix| validate_name_prefix("classmap_prefix", prefix))
            .transpose()?;

        // As in PHP Strauss, the namespace prefix is inferred first (and may
        // use an explicit classmap prefix), then the classmap prefix (which
        // may use the namespace prefix).
        let namespace_prefix = explicit_namespace_prefix
            .or_else(|| infer_namespace_prefix(&composer, explicit_classmap_prefix.as_deref()));

        let classmap_prefix = explicit_classmap_prefix
            .or_else(|| infer_classmap_prefix(&composer, namespace_prefix.as_deref()));

        let constant_prefix = match raw.constant_prefix.as_deref() {
            None | Some("") => None,
            Some(prefix) => Some(validate_name_prefix("constant_prefix", prefix)?),
        };

        let functions_prefix = resolve_functions_prefix(raw.functions_prefix, &classmap_prefix)?;

        let packages = match raw.packages {
            Some(packages) if !packages.is_empty() => {
                for (index, package) in packages.iter().enumerate() {
                    require_non_empty(&format!("packages[{index}]"), package)?;
                }

                packages
            }
            _ => {
                let dev = if raw.packages_includes_dev.unwrap_or(false) {
                    composer.require_dev.0.as_slice()
                } else {
                    &[]
                };

                let mut packages: Vec<String> = Vec::new();

                for (name, _) in composer.require.0.iter().chain(dev) {
                    if !packages.contains(name) {
                        packages.push(name.clone());
                    }
                }

                packages
            }
        };

        let classmap_output = raw
            .classmap_output
            .unwrap_or_else(|| !autoloads_directory(&composer.autoload, &target_directory));

        let mut namespace_replacement_patterns = Vec::new();

        for (index, (pattern, replacement)) in raw
            .namespace_replacement_patterns
            .map(|patterns| patterns.0)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            require_non_empty(
                &format!("namespace_replacement_patterns[{index}]"),
                &pattern,
            )?;

            namespace_replacement_patterns.push(NamespaceReplacement {
                pattern: PhpPattern::literal_or_regex(&pattern)
                    .map_err(|message| invalid("namespace_replacement_patterns", message))?,
                replacement: Replacement::parse(&replacement),
            });
        }

        let exclude_constants = raw.exclude_constants.unwrap_or_default();

        for (index, constant) in exclude_constants.constants.iter().enumerate() {
            if !is_identifier(constant) {
                return Err(invalid(
                    format!("exclude_constants.constants[{index}]"),
                    format!("`{constant}` is not a valid constant name"),
                ));
            }
        }

        let exclude_presets = resolve_presets(raw.exclude_presets)?;

        let exclude_from_string_rename = raw.exclude_from_string_rename.unwrap_or_default();

        for (index, function) in exclude_from_string_rename.functions.iter().enumerate() {
            if !is_identifier(function) {
                return Err(invalid(
                    format!("exclude_from_string_rename.functions[{index}]"),
                    format!("`{function}` is not a valid function name"),
                ));
            }
        }

        Ok(Config {
            project_dir: project_dir.to_path_buf(),
            vendor_directory: composer
                .config
                .vendor_dir
                .as_deref()
                .map(|directory| directory.trim_matches('/').to_string())
                .filter(|directory| !directory.is_empty())
                .unwrap_or_else(|| DEFAULT_VENDOR_DIRECTORY.to_string()),
            target_directory,
            namespace_prefix,
            classmap_prefix,
            constant_prefix,
            functions_prefix,
            packages,
            update_call_sites: resolve_update_call_sites(raw.update_call_sites)?,
            include_root_autoload: raw.include_root_autoload.unwrap_or(false),
            optimize_autoloader: raw.optimize_autoloader.unwrap_or(true),
            parallel: resolve_parallel(raw.parallel)?,
            override_autoload: raw.override_autoload.unwrap_or_default(),
            exclude_from_copy: resolve_exclusions(
                "exclude_from_copy",
                raw.exclude_from_copy.unwrap_or_default(),
            )?,
            exclude_from_prefix: resolve_prefix_exclusions(
                "exclude_from_prefix",
                raw.exclude_from_prefix.unwrap_or_default(),
            )?,
            exclude_files_from_update: resolve_exclusions(
                "exclude_files_from_update",
                raw.exclude_files_from_update.unwrap_or_default(),
            )?,
            exclude_constants: ConstantExclusions {
                exclusions: resolve_exclusions(
                    "exclude_constants",
                    RawExclusions {
                        packages: exclude_constants.packages,
                        namespaces: exclude_constants.namespaces,
                        file_patterns: exclude_constants.file_patterns,
                    },
                )?,
                constants: exclude_constants.constants,
            },
            exclude_presets,
            exclude_from_string_rename: StringRenameExclusions {
                functions: exclude_from_string_rename.functions,
            },
            namespace_replacement_patterns,
            delete_vendor_packages: raw.delete_vendor_packages.unwrap_or(false),
            delete_vendor_files: raw.delete_vendor_files.unwrap_or(false),
            exclude_git_files: raw.exclude_git_files.unwrap_or(true),
            include_modified_date: raw.include_modified_date.unwrap_or(true),
            include_author: raw.include_author.unwrap_or(true),
            include_autoloader: raw.include_autoloader.unwrap_or(false),
            classmap_output,
        })
    }
}

/// The `classmap_prefix` PHP Strauss would infer from this `composer.json`, if
/// it isn't the one strauss-rs infers. Setting it keeps class names as they
/// were after moving from PHP Strauss.
pub fn php_strauss_classmap_prefix(composer_json: &str) -> Option<String> {
    let composer: ComposerJson = serde_json::from_str(composer_json).ok()?;

    infer::infer_php_strauss_classmap_prefix(&composer)
}

fn read_optional(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests;
