//! The options as written in `strauss.json`, before defaults, inference and validation.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::config::composer::{Autoload, Ordered};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {
    /// Allows `"$schema"` to be set for editor support.
    #[serde(rename = "$schema")]
    pub(super) schema: Option<String>,

    pub(super) target_directory: Option<String>,
    pub(super) namespace_prefix: Option<String>,
    pub(super) classmap_prefix: Option<String>,
    pub(super) constant_prefix: Option<String>,
    #[serde(alias = "function_prefix")]
    pub(super) functions_prefix: Option<Value>,
    pub(super) packages: Option<Vec<String>>,
    pub(super) packages_includes_dev: Option<bool>,
    pub(super) update_call_sites: Option<Value>,
    pub(super) include_root_autoload: Option<bool>,
    pub(super) optimize_autoloader: Option<bool>,
    pub(super) parallel: Option<Value>,
    pub(super) override_autoload: Option<BTreeMap<String, Autoload>>,
    pub(super) exclude_from_copy: Option<RawExclusions>,
    pub(super) exclude_from_prefix: Option<RawPrefixExclusions>,
    pub(super) exclude_files_from_update: Option<RawExclusions>,
    pub(super) exclude_constants: Option<RawConstantExclusions>,
    pub(super) exclude_presets: Option<Vec<String>>,
    pub(super) exclude_from_string_rename: Option<RawStringRenameExclusions>,
    pub(super) namespace_replacement_patterns: Option<Ordered<String>>,
    pub(super) delete_vendor_packages: Option<bool>,
    pub(super) delete_vendor_files: Option<bool>,
    pub(super) exclude_git_files: Option<bool>,
    pub(super) include_modified_date: Option<bool>,
    pub(super) include_author: Option<bool>,
    pub(super) include_autoloader: Option<bool>,
    pub(super) classmap_output: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawExclusions {
    #[serde(default)]
    pub(super) packages: Vec<String>,
    #[serde(default)]
    pub(super) namespaces: Vec<String>,
    #[serde(default)]
    pub(super) file_patterns: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPrefixExclusions {
    #[serde(default)]
    pub(super) packages: Vec<String>,
    #[serde(default)]
    pub(super) namespaces: Vec<String>,
    #[serde(default)]
    pub(super) file_patterns: Vec<String>,
    #[serde(default)]
    pub(super) functions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConstantExclusions {
    #[serde(default)]
    pub(super) packages: Vec<String>,
    #[serde(default)]
    pub(super) namespaces: Vec<String>,
    #[serde(default)]
    pub(super) file_patterns: Vec<String>,
    #[serde(default)]
    pub(super) constants: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawStringRenameExclusions {
    #[serde(default)]
    pub(super) functions: Vec<String>,
}
