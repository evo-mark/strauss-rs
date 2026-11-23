//! What package enumeration finds.

use std::fmt;
use std::path::PathBuf;

use serde::{Serialize, Serializer};

use crate::config::Autoload;

/// A package to copy and prefix, or one of its dependencies.
#[derive(Debug, Serialize)]
pub struct Package {
    /// The package name, as Composer spells it: `league/flysystem`.
    pub name: String,

    pub version: Option<String>,

    /// The Composer package type: `library`, `wordpress-plugin`, `metapackage`…
    #[serde(rename = "type")]
    pub package_type: Option<String>,

    /// The directory holding the package's files, with symlinks resolved.
    /// `None` for a metapackage, which has no files, and for a package whose
    /// directory doesn't exist.
    pub path: Option<PathBuf>,

    /// The package's autoload rules, or the project's `override_autoload`
    /// for it.
    pub autoload: Autoload,

    /// Whether `autoload` came from the project's `override_autoload`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub autoload_overridden: bool,

    /// The packages it requires, without platform requirements (`php`,
    /// `ext-json`, …).
    pub requires: Vec<String>,

    /// The names it `provide`s or `replace`s.
    pub provides: Vec<String>,

    /// The packages it suggests.
    pub suggests: Vec<String>,

    pub license: Vec<String>,

    /// The packages in this enumeration that it depends on: those it requires
    /// directly, and those that `provide` or `replace` something it requires.
    pub dependencies: Vec<String>,

    /// Not to be prefixed: a non-dev package outside the dependency tree of
    /// `packages`, added because the target is the vendor directory. Its files
    /// are updated where they use the prefixed names, and nothing it declares
    /// is renamed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unprefixed: bool,
}

/// Why a package was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Required, but not in `vendor/composer/installed.json`.
    NotInstalled,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SkipReason::NotInstalled => formatter.write_str("not installed"),
        }
    }
}

impl Serialize for SkipReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[derive(Debug, Serialize)]
pub struct SkippedPackage {
    pub name: String,
    pub reason: SkipReason,
}

/// A warning about a package whose files can't be listed.
#[derive(Debug, Serialize)]
pub struct PackageWarning {
    pub package: String,
    pub message: String,
}

/// The packages to process, in the order PHP Strauss visits them: each
/// package, then its requirements, depth first.
#[derive(Debug, Default, Serialize)]
pub struct PackageSet {
    pub packages: Vec<Package>,

    /// Required packages that were left out.
    pub skipped: Vec<SkippedPackage>,

    /// Packages that were found but have a problem.
    pub warnings: Vec<PackageWarning>,
}

impl PackageSet {
    /// The packages to prefix: all but those added as [`Package::unprefixed`].
    pub fn prefixed(&self) -> impl Iterator<Item = &Package> {
        self.packages.iter().filter(|package| !package.unprefixed)
    }

    pub fn get(&self, name: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|package| package.name.eq_ignore_ascii_case(name))
    }
}

/// Why a file won't be copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyExclusion {
    /// `target_directory` is the vendor directory, so there is nothing to
    /// copy; the files are changed where they are.
    TargetIsVendor,
    /// The package is in `exclude_from_copy.packages`.
    PackageExcluded,
    /// The file matches this `exclude_from_copy.file_patterns` entry.
    PatternMatched(String),
    /// The file declares a namespace in this `exclude_from_copy.namespaces`
    /// entry. Only known once the file's symbols have been found.
    NamespaceExcluded(String),
}

impl fmt::Display for CopyExclusion {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CopyExclusion::TargetIsVendor => {
                formatter.write_str("the target directory is the vendor directory")
            }
            CopyExclusion::PackageExcluded => {
                formatter.write_str("the package is in exclude_from_copy.packages")
            }
            CopyExclusion::PatternMatched(pattern) => {
                write!(
                    formatter,
                    "it matches exclude_from_copy.file_patterns `{pattern}`"
                )
            }
            CopyExclusion::NamespaceExcluded(namespace) => write!(
                formatter,
                "it declares a namespace in exclude_from_copy.namespaces `{namespace}`"
            ),
        }
    }
}

impl Serialize for CopyExclusion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A file belonging to a package.
#[derive(Debug, Serialize)]
pub struct PackageFile {
    /// The package it belongs to.
    pub package: String,

    /// Where the file is now.
    pub source_path: PathBuf,

    /// The path inside the package: `src/Client.php`.
    pub package_relative_path: String,

    /// The path as patterns and symbols see it: `league/flysystem/src/Client.php`.
    pub vendor_relative_path: String,

    /// Where the file is copied to.
    pub target_path: PathBuf,

    /// Whether the file is copied to `target_path`.
    pub copy: bool,

    /// Why it isn't copied, when `copy` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_copied_because: Option<CopyExclusion>,
}
