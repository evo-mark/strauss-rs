//! The resolved configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::config::composer::Autoload;
use crate::php_pattern::{PhpPattern, Replacement};

/// The resolved configuration: every option set, either by the user or by
/// the same defaults and inference as PHP Strauss.
///
/// Serializing it gives the options as `strauss.json` would write them, with
/// every default and inferred value filled in, so the output can be saved as a
/// `strauss.json` that produces the same configuration.
#[derive(Debug, Serialize)]
pub struct Config {
    /// The directory containing `strauss.json` and `composer.json`.
    #[serde(skip)]
    pub project_dir: PathBuf,

    /// Composer's vendor directory, relative to the project. Read from
    /// `config.vendor-dir` in `composer.json`. Default `vendor`.
    #[serde(skip)]
    pub vendor_directory: String,

    /// `target_directory`: where the prefixed files are written, relative to
    /// the project, without leading or trailing slashes. Default
    /// `vendor-prefixed`.
    pub target_directory: String,

    /// `namespace_prefix`: prepended to every namespace, without leading or
    /// trailing backslashes (`Acme\Plugin`). Inferred from `composer.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_prefix: Option<String>,

    /// `classmap_prefix`: prepended to classes in the global namespace
    /// (`Acme_Plugin_`). Inferred from `composer.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classmap_prefix: Option<String>,

    /// `constant_prefix`: prepended to constants declared with `define()`.
    /// `None` leaves constants alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constant_prefix: Option<String>,

    /// `functions_prefix`: prepended to global functions. Defaults to the
    /// lower-cased `classmap_prefix`; `false` or `""` disables it.
    #[serde(serialize_with = "serialize_functions_prefix")]
    pub functions_prefix: Option<String>,

    /// `packages`: the packages to process. Defaults to the `require` keys of
    /// `composer.json`, which includes platform requirements such as `php` and
    /// `ext-json`; those won't match an installed package. With
    /// `packages_includes_dev`, the `require-dev` keys are added too.
    pub packages: Vec<String>,

    /// `update_call_sites`.
    pub update_call_sites: UpdateCallSites,

    /// `include_root_autoload`: include the project's own `autoload` section
    /// in the generated autoloader. Default `false`.
    pub include_root_autoload: bool,

    /// `optimize_autoloader`: force an optimised, classmap-authoritative
    /// autoloader. Default `true`.
    pub optimize_autoloader: bool,

    /// `parallel`. Default `auto`: one worker per available CPU.
    pub parallel: Parallel,

    /// `override_autoload`: autoload settings that replace a package's own,
    /// keyed by package name.
    pub override_autoload: BTreeMap<String, Autoload>,

    /// `exclude_from_copy`: packages, namespaces and files that are not copied
    /// into `target_directory` at all.
    pub exclude_from_copy: Exclusions,

    /// `exclude_from_prefix`: symbols defined here are not renamed (though
    /// where they are used is still updated).
    pub exclude_from_prefix: Exclusions,

    /// `exclude_files_from_update`: files, packages and namespaces in which
    /// no changes are made.
    pub exclude_files_from_update: Exclusions,

    /// `exclude_constants`: like `exclude_from_prefix`, for constants only.
    pub exclude_constants: ConstantExclusions,

    /// `exclude_presets`: sets of symbols that something other than PHP
    /// provides, which are never renamed. Default `["wordpress"]`.
    pub exclude_presets: Vec<String>,

    /// `exclude_from_string_rename`: symbols that are renamed in code but left
    /// alone inside strings.
    pub exclude_from_string_rename: StringRenameExclusions,

    /// `namespace_replacement_patterns`, in priority order: the first pattern
    /// that changes a namespace wins.
    #[serde(serialize_with = "serialize_replacement_patterns")]
    pub namespace_replacement_patterns: Vec<NamespaceReplacement>,

    /// `delete_vendor_packages`: delete the packages' vendor directories after
    /// processing. Default `false`.
    pub delete_vendor_packages: bool,

    /// `delete_vendor_files`: delete the copied files from the packages'
    /// vendor directories after processing. Default `false`. Deprecated in
    /// favour of `delete_vendor_packages`, but still honoured.
    pub delete_vendor_files: bool,

    /// `exclude_git_files`: skip files that wouldn't be in the package's
    /// distributed archive (`.git`, `.gitignore`d and `export-ignore` files).
    /// Default `true`.
    pub exclude_git_files: bool,

    /// `include_modified_date`: put a date in the header written to modified
    /// files. Default `true`.
    pub include_modified_date: bool,

    /// `include_author`: put the author in the header written to modified
    /// files. Default `true`.
    pub include_author: bool,

    /// `include_autoloader`: after `prefix`, make the vendor directory's
    /// `autoload.php` load the prefixed packages' autoloader (what the
    /// `include-autoloader` command does). Default `false`.
    pub include_autoloader: bool,

    /// `classmap_output`: write `autoload-classmap.php` and `autoload.php`.
    /// Inferred: `false` when `target_directory` is in the project's
    /// `autoload`, `true` otherwise.
    pub classmap_output: bool,
}

/// `update_call_sites`: which project files to update where they use the
/// prefixed symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCallSites {
    /// `false` (the default): don't update the project's files.
    Disabled,
    /// `true`: the files and directories in the project's `autoload`.
    FromAutoload,
    /// An explicit list of files and directories, relative to the project.
    Paths(Vec<String>),
}

impl Serialize for UpdateCallSites {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            UpdateCallSites::Disabled => serializer.serialize_bool(false),
            UpdateCallSites::FromAutoload => serializer.serialize_bool(true),
            UpdateCallSites::Paths(paths) => paths.serialize(serializer),
        }
    }
}

/// The command line form: `true`, `false`, or a comma-separated list of files
/// and directories, as in `--updateCallSites=includes,templates`.
impl FromStr for UpdateCallSites {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();

        if value.eq_ignore_ascii_case("true") {
            return Ok(UpdateCallSites::FromAutoload);
        }

        if value.eq_ignore_ascii_case("false") {
            return Ok(UpdateCallSites::Disabled);
        }

        let paths: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(String::from)
            .collect();

        if paths.is_empty() {
            return Err(
                "expected `true`, `false`, or a comma-separated list of files and directories"
                    .to_string(),
            );
        }

        Ok(UpdateCallSites::Paths(paths))
    }
}

/// `parallel`: how many worker processes analyse files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parallel {
    /// `"auto"`, `true` or `0`: one worker per available CPU.
    Auto,
    /// `false` or `1`: do everything in one process.
    Disabled,
    /// `N`: at most `N` workers.
    Workers(usize),
}

impl Parallel {
    /// `0` is automatic (one worker per available CPU), `1` is a single
    /// worker, and anything larger caps the number of workers.
    pub fn from_workers(workers: usize) -> Self {
        match workers {
            0 => Parallel::Auto,
            1 => Parallel::Disabled,
            workers => Parallel::Workers(workers),
        }
    }
}

impl Serialize for Parallel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Parallel::Auto => serializer.serialize_str("auto"),
            Parallel::Disabled => serializer.serialize_bool(false),
            Parallel::Workers(workers) => serializer.serialize_u64(*workers as u64),
        }
    }
}

/// The command line form: `auto`, `true`, `false`, or a number of workers, as
/// in `--parallel=4`.
impl FromStr for Parallel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();

        if value.eq_ignore_ascii_case("auto") || value.eq_ignore_ascii_case("true") {
            return Ok(Parallel::Auto);
        }

        if value.eq_ignore_ascii_case("false") {
            return Ok(Parallel::Disabled);
        }

        value
            .parse::<usize>()
            .map(Parallel::from_workers)
            .map_err(|_| "expected `auto`, `true`, `false`, or a whole number".to_string())
    }
}

/// The `packages`, `namespaces` and `file_patterns` lists shared by the
/// `exclude_*` options.
#[derive(Debug, Default, Serialize)]
pub struct Exclusions {
    /// Package names, e.g. `league/flysystem`.
    pub packages: Vec<String>,
    /// Namespaces, matched exactly, without leading or trailing backslashes.
    pub namespaces: Vec<String>,
    /// Regular expressions matched against the path relative to the vendor
    /// directory.
    pub file_patterns: Vec<PhpPattern>,
    /// Function names that are never prefixed, without a leading backslash.
    /// Only `exclude_from_prefix` takes them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ConstantExclusions {
    #[serde(flatten)]
    pub exclusions: Exclusions,
    /// Constant names that are never prefixed, e.g. `WP_PLUGIN_DIR`.
    pub constants: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct StringRenameExclusions {
    /// Global function names whose name is left unchanged inside strings,
    /// such as `$array['value']`.
    pub functions: Vec<String>,
}

#[derive(Debug)]
pub struct NamespaceReplacement {
    pub pattern: PhpPattern,
    pub replacement: Replacement,
}

/// `false` is how `strauss.json` says "no function prefix"; leaving the option
/// out would mean "use the default".
fn serialize_functions_prefix<S: Serializer>(
    prefix: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match prefix {
        Some(prefix) => serializer.serialize_str(prefix),
        None => serializer.serialize_bool(false),
    }
}

/// An object, in priority order.
fn serialize_replacement_patterns<S: Serializer>(
    patterns: &[NamespaceReplacement],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(patterns.len()))?;

    for entry in patterns {
        map.serialize_entry(entry.pattern.source(), &entry.replacement)?;
    }

    map.end()
}
