//! Checking, normalising and resolving the options as written.

use serde_json::Value;

use crate::config::CONFIG_FILE;
use crate::config::error::{ConfigError, invalid};
use crate::config::model::{Exclusions, Parallel, UpdateCallSites};
use crate::config::raw::{RawExclusions, RawPrefixExclusions};
use crate::php_pattern::PhpPattern;
use crate::symbols::presets::{DEFAULT_PRESETS, PRESET_NAMES};

/// Options whose value is a JSON object.
pub(super) const OBJECT_OPTIONS: &[&str] = &[
    "exclude_from_copy",
    "exclude_from_prefix",
    "exclude_files_from_update",
    "exclude_constants",
    "exclude_from_string_rename",
    "namespace_replacement_patterns",
    "override_autoload",
];

/// Serde reads a JSON array as a struct's fields in order, so
/// `"exclude_from_copy": ["a/b"]` would be taken as `packages: "a/b"` and fail
/// with a confusing message (or, worse, `[["a/b"]]` would be accepted). Check
/// that objects are objects first. Syntax errors are left for the real parse,
/// which reports their line and column.
pub(super) fn check_shape(json: &str) -> Result<(), ConfigError> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Ok(());
    };

    let Value::Object(options) = &value else {
        return Err(ConfigError::Parse {
            file: CONFIG_FILE,
            message: "must contain a JSON object".to_string(),
        });
    };

    for key in OBJECT_OPTIONS {
        match options.get(*key) {
            None | Some(Value::Null) | Some(Value::Object(_)) => {}
            Some(_) => return Err(invalid(*key, "must be an object")),
        }
    }

    if let Some(Value::Object(packages)) = options.get("override_autoload") {
        for (package, autoload) in packages {
            if !autoload.is_object() {
                return Err(invalid(
                    format!("override_autoload.{package}"),
                    "must be an object",
                ));
            }
        }
    }

    Ok(())
}

pub(super) fn require_non_empty(key: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(invalid(key, "must not be empty"));
    }

    Ok(())
}

/// A PHP identifier: letters, digits and underscores (and bytes above 0x7f),
/// not starting with a digit.
pub(super) fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();

    match characters.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() || !first.is_ascii() => {}
        _ => return false,
    }

    characters.all(|character| {
        character == '_' || character.is_ascii_alphanumeric() || !character.is_ascii()
    })
}

/// `target_directory` has no leading or trailing slash and must stay inside
/// the project: it is written to, and with `delete_vendor_*` set, deleted
/// around. Parent (`..`) segments and drive letters are rejected.
pub(super) fn normalise_target_directory(directory: &str) -> Result<String, ConfigError> {
    let trimmed = directory.trim_matches(|character| character == '/' || character == '\\');

    if trimmed.is_empty() {
        return Err(invalid("target_directory", "must not be empty"));
    }

    // A leading slash is allowed: as in Strauss, the directory is joined onto
    // the project path, so `/lib/prefixed` means `<project>/lib/prefixed`.
    let escapes_project = directory.as_bytes().get(1) == Some(&b':')
        || trimmed
            .split(['/', '\\'])
            .any(|segment| segment == ".." || segment == ".");

    if escapes_project {
        return Err(invalid(
            "target_directory",
            format!("`{directory}` must be a path inside the project"),
        ));
    }

    Ok(trimmed.replace('\\', "/"))
}

/// Backslashes are trimmed from both ends (Strauss does the same when it
/// reads the prefix), and each segment must be a valid PHP name.
pub(super) fn validate_namespace_prefix(key: &str, prefix: &str) -> Result<String, ConfigError> {
    let trimmed = prefix.trim_matches('\\');

    if trimmed.is_empty() {
        return Err(invalid(key, "must not be empty"));
    }

    if let Some(segment) = trimmed.split('\\').find(|segment| !is_identifier(segment)) {
        return Err(invalid(
            key,
            format!("`{prefix}` is not a valid namespace: `{segment}` is not a valid name"),
        ));
    }

    Ok(trimmed.to_string())
}

/// Used for `classmap_prefix` and `constant_prefix`, which are glued onto the
/// front of a name (`Acme_Plugin_` + `Client`).
pub(super) fn validate_name_prefix(key: &str, prefix: &str) -> Result<String, ConfigError> {
    if prefix.is_empty() {
        return Err(invalid(key, "must not be empty"));
    }

    if !is_identifier(prefix) {
        let hint = if key == "classmap_prefix" && prefix.contains('\\') {
            " (a namespace belongs in `namespace_prefix`)"
        } else {
            ""
        };

        return Err(invalid(
            key,
            format!(
                "`{prefix}` can only contain letters, numbers and underscores, and can't start with a number{hint}"
            ),
        ));
    }

    Ok(prefix.to_string())
}

/// `exclude_presets` names must be known presets. Default: all of them.
pub(super) fn resolve_presets(raw: Option<Vec<String>>) -> Result<Vec<String>, ConfigError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_PRESETS
            .iter()
            .map(|name| name.to_string())
            .collect());
    };

    let mut presets: Vec<String> = Vec::new();

    for (index, preset) in raw.iter().enumerate() {
        let name = preset.trim().to_ascii_lowercase();

        if !PRESET_NAMES.contains(&name.as_str()) {
            return Err(invalid(
                format!("exclude_presets[{index}]"),
                format!(
                    "`{preset}` is not a preset; the presets are {}",
                    PRESET_NAMES.join(", ")
                ),
            ));
        }

        if !presets.contains(&name) {
            presets.push(name);
        }
    }

    Ok(presets)
}

pub(super) fn resolve_exclusions(key: &str, raw: RawExclusions) -> Result<Exclusions, ConfigError> {
    for (index, package) in raw.packages.iter().enumerate() {
        require_non_empty(&format!("{key}.packages[{index}]"), package)?;
    }

    let mut namespaces = Vec::new();

    for (index, namespace) in raw.namespaces.iter().enumerate() {
        // `Acme\Lib\` is how a PSR-4 key is written; namespaces are matched
        // without the surrounding backslashes.
        let trimmed = namespace.trim_matches('\\');

        require_non_empty(&format!("{key}.namespaces[{index}]"), trimmed)?;
        namespaces.push(trimmed.to_string());
    }

    let mut file_patterns = Vec::new();

    for (index, pattern) in raw.file_patterns.iter().enumerate() {
        require_non_empty(&format!("{key}.file_patterns[{index}]"), pattern)?;

        file_patterns.push(
            PhpPattern::regex(pattern)
                .map_err(|message| invalid(format!("{key}.file_patterns[{index}]"), message))?,
        );
    }

    Ok(Exclusions {
        packages: raw.packages,
        namespaces,
        file_patterns,
        functions: Vec::new(),
    })
}

/// `exclude_from_prefix`: the lists the others have, and function names.
pub(super) fn resolve_prefix_exclusions(
    key: &str,
    raw: RawPrefixExclusions,
) -> Result<Exclusions, ConfigError> {
    let mut functions = Vec::new();

    for (index, function) in raw.functions.iter().enumerate() {
        // `\dd` and `dd` are the same function.
        let trimmed = function.trim_start_matches('\\');

        require_non_empty(&format!("{key}.functions[{index}]"), trimmed)?;
        functions.push(trimmed.to_string());
    }

    let mut exclusions = resolve_exclusions(
        key,
        RawExclusions {
            packages: raw.packages,
            namespaces: raw.namespaces,
            file_patterns: raw.file_patterns,
        },
    )?;

    exclusions.functions = functions;

    Ok(exclusions)
}

pub(super) fn resolve_update_call_sites(
    value: Option<Value>,
) -> Result<UpdateCallSites, ConfigError> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(UpdateCallSites::Disabled),
        Some(Value::Bool(true)) => Ok(UpdateCallSites::FromAutoload),
        Some(Value::Array(items)) => {
            let mut paths = Vec::new();

            for (index, item) in items.into_iter().enumerate() {
                let key = format!("update_call_sites[{index}]");

                match item {
                    Value::String(path) => {
                        require_non_empty(&key, &path)?;
                        paths.push(path);
                    }
                    _ => return Err(invalid(key, "must be a string")),
                }
            }

            // An empty list disables updating, as in Strauss.
            if paths.is_empty() {
                Ok(UpdateCallSites::Disabled)
            } else {
                Ok(UpdateCallSites::Paths(paths))
            }
        }
        Some(_) => Err(invalid(
            "update_call_sites",
            "must be true, false, or an array of files and directories",
        )),
    }
}

const PARALLEL_EXPECTED: &str = "must be \"auto\", true, false, or a whole number";

pub(super) fn resolve_parallel(value: Option<Value>) -> Result<Parallel, ConfigError> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(true)) => Ok(Parallel::Auto),
        Some(Value::Bool(false)) => Ok(Parallel::Disabled),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("auto") => Ok(Parallel::Auto),
        Some(Value::Number(number)) => match number.as_u64() {
            Some(workers) => Ok(Parallel::from_workers(workers as usize)),
            None => Err(invalid("parallel", PARALLEL_EXPECTED)),
        },
        Some(_) => Err(invalid("parallel", PARALLEL_EXPECTED)),
    }
}

/// Unset (or `null`) defaults to the lower-cased classmap prefix. `false` or
/// `""` turns function prefixing off.
pub(super) fn resolve_functions_prefix(
    value: Option<Value>,
    classmap_prefix: &Option<String>,
) -> Result<Option<String>, ConfigError> {
    match value {
        None | Some(Value::Null) => {
            Ok(classmap_prefix.as_ref().map(|prefix| prefix.to_lowercase()))
        }
        Some(Value::Bool(false)) => Ok(None),
        Some(Value::String(prefix)) if prefix.is_empty() => Ok(None),
        Some(Value::String(prefix)) => validate_name_prefix("functions_prefix", &prefix).map(Some),
        Some(_) => Err(invalid(
            "functions_prefix",
            "must be a string, or false to disable function prefixing",
        )),
    }
}
