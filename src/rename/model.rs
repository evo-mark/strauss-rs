//! What will be renamed, and to what.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Serialize, Serializer};

use crate::symbols::{SymbolKind, SymbolSource};

/// Why a symbol is, or isn't, renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    // ---- renamed ----
    /// The symbol is autoloaded by its package, so other code finds it by its
    /// name.
    Autoloaded,
    /// Its namespace matches a `namespace_replacement_patterns` entry.
    MatchesNamespaceReplacementPattern,
    /// Nothing autoloads it, but it is in a namespace that is renamed. The
    /// namespace is renamed in every file that declares it, so it moves with it.
    FollowsNamespace,

    // ---- not renamed ----
    /// Nothing autoloads it, so nothing outside its package needs to find it.
    NotAutoloaded,
    /// Its package is in `exclude_from_copy.packages`.
    PackageExcludedFromCopy(String),
    /// Its package is in `exclude_from_prefix.packages`.
    PackageExcludedFromPrefix(String),
    /// Its namespace is, or is within, this `exclude_from_prefix.namespaces`
    /// entry.
    NamespaceExcludedFromPrefix(String),
    /// A file it is declared in matches this `exclude_from_prefix.file_patterns`
    /// entry.
    FileExcludedFromPrefix { pattern: String, file: String },
    /// It is this `exclude_from_prefix.functions` entry.
    FunctionExcludedFromPrefix(String),
    /// Its package is in `exclude_constants.packages`.
    ConstantPackageExcluded(String),
    /// Its namespace is, or is within, this `exclude_constants.namespaces`
    /// entry.
    ConstantNamespaceExcluded(String),
    /// A file it is declared in matches this `exclude_constants.file_patterns`
    /// entry.
    ConstantFileExcluded { pattern: String, file: String },
    /// It is in `exclude_constants.constants`.
    ConstantExcluded,
    /// Every file that declares it is left out of the copy, so renaming it
    /// would point other code at a name that doesn't exist.
    OnlyInFilesNotCopied,
    /// It is in a namespace that isn't renamed.
    NamespaceNotRenamed(String),
    /// Its name already has the prefix.
    AlreadyPrefixed,
    /// No `namespace_replacement_patterns` entry matched and there is no
    /// `namespace_prefix`.
    NoNamespaceReplacement,
    /// This option isn't set, so there is nothing to rename it with.
    NoPrefixConfigured(&'static str),
}

impl Reason {
    /// A short name for the kind of reason, without the particulars.
    pub fn category(&self) -> &'static str {
        match self {
            Reason::Autoloaded => "autoloaded",
            Reason::MatchesNamespaceReplacementPattern => "namespace_replacement_pattern",
            Reason::FollowsNamespace => "follows_namespace",
            Reason::NotAutoloaded => "not_autoloaded",
            Reason::PackageExcludedFromCopy(_) => "package_excluded_from_copy",
            Reason::PackageExcludedFromPrefix(_) => "package_excluded_from_prefix",
            Reason::NamespaceExcludedFromPrefix(_) => "namespace_excluded_from_prefix",
            Reason::FileExcludedFromPrefix { .. } => "file_excluded_from_prefix",
            Reason::FunctionExcludedFromPrefix(_) => "function_excluded_from_prefix",
            Reason::ConstantPackageExcluded(_) => "constant_package_excluded",
            Reason::ConstantNamespaceExcluded(_) => "constant_namespace_excluded",
            Reason::ConstantFileExcluded { .. } => "constant_file_excluded",
            Reason::ConstantExcluded => "constant_excluded",
            Reason::OnlyInFilesNotCopied => "only_in_files_not_copied",
            Reason::NamespaceNotRenamed(_) => "namespace_not_renamed",
            Reason::AlreadyPrefixed => "already_prefixed",
            Reason::NoNamespaceReplacement => "no_namespace_replacement",
            Reason::NoPrefixConfigured(_) => "no_prefix_configured",
        }
    }

    /// Whether this is a reason to rename.
    pub fn is_rename(&self) -> bool {
        matches!(
            self,
            Reason::Autoloaded
                | Reason::MatchesNamespaceReplacementPattern
                | Reason::FollowsNamespace
        )
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Reason::Autoloaded => formatter.write_str("it is autoloaded"),
            Reason::MatchesNamespaceReplacementPattern => {
                formatter.write_str("its namespace matches namespace_replacement_patterns")
            }
            Reason::FollowsNamespace => {
                formatter.write_str("it is in a namespace that is renamed, so it moves with it")
            }
            Reason::NotAutoloaded => formatter.write_str("it is not autoloaded"),
            Reason::PackageExcludedFromCopy(package) => {
                write!(
                    formatter,
                    "package {package} is in exclude_from_copy.packages"
                )
            }
            Reason::PackageExcludedFromPrefix(package) => {
                write!(
                    formatter,
                    "package {package} is in exclude_from_prefix.packages"
                )
            }
            Reason::NamespaceExcludedFromPrefix(namespace) => {
                write!(
                    formatter,
                    "it is in exclude_from_prefix.namespaces `{namespace}`"
                )
            }
            Reason::FileExcludedFromPrefix { pattern, file } => write!(
                formatter,
                "{file} matches exclude_from_prefix.file_patterns `{pattern}`"
            ),
            Reason::FunctionExcludedFromPrefix(function) => {
                write!(
                    formatter,
                    "it is in exclude_from_prefix.functions `{function}`"
                )
            }
            Reason::ConstantPackageExcluded(package) => {
                write!(
                    formatter,
                    "package {package} is in exclude_constants.packages"
                )
            }
            Reason::ConstantNamespaceExcluded(namespace) => {
                write!(
                    formatter,
                    "it is in exclude_constants.namespaces `{namespace}`"
                )
            }
            Reason::ConstantFileExcluded { pattern, file } => write!(
                formatter,
                "{file} matches exclude_constants.file_patterns `{pattern}`"
            ),
            Reason::ConstantExcluded => formatter.write_str("it is in exclude_constants.constants"),
            Reason::OnlyInFilesNotCopied => {
                formatter.write_str("it is only declared in files that are not copied")
            }
            Reason::NamespaceNotRenamed(namespace) => {
                write!(formatter, "its namespace {namespace} is not renamed")
            }
            Reason::AlreadyPrefixed => formatter.write_str("it already has the prefix"),
            Reason::NoNamespaceReplacement => formatter.write_str(
                "no namespace_replacement_patterns entry matches and there is no namespace_prefix",
            ),
            Reason::NoPrefixConfigured(option) => write!(formatter, "{option} is not set"),
        }
    }
}

impl Serialize for Reason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Which autoload rule a namespace comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Psr {
    #[serde(rename = "psr-0")]
    Psr0,
    #[serde(rename = "psr-4")]
    Psr4,
}

/// A symbol and what will happen to its name.
#[derive(Debug, Serialize)]
pub struct PlannedSymbol {
    pub kind: SymbolKind,

    /// The full name, without a leading backslash.
    pub name: String,

    /// The namespace it is in; empty for the global namespace.
    pub namespace: String,

    /// The packages that declare it, or whose autoload rules name it.
    pub packages: Vec<String>,

    /// The files that declare it.
    #[serde(skip)]
    pub sources: Vec<SymbolSource>,

    /// Whether its package autoloads it.
    pub autoloaded: bool,

    /// Whether it will be renamed. This is exactly when `replacement` is set.
    pub rename: bool,

    /// The new full name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,

    pub reason: Reason,

    /// Whether the name is also replaced inside strings. Only a global function
    /// in `exclude_from_string_rename.functions` is left alone there.
    #[serde(skip_serializing_if = "is_true")]
    pub replace_in_strings: bool,

    /// Set for a namespace that comes from a package's autoload rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub psr: Option<Psr>,

    /// Whether the symbol is to be renamed, before the replacement name is
    /// worked out: autoloaded or matching a pattern, and not excluded.
    #[serde(skip)]
    pub(super) marked: bool,
}

fn is_true(value: &bool) -> bool {
    *value
}

impl PlannedSymbol {
    /// The name without its namespace.
    pub fn local_name(&self) -> &str {
        match self.kind {
            SymbolKind::Namespace => &self.name,
            _ => self.name.rsplit('\\').next().unwrap_or(&self.name),
        }
    }

    pub fn is_global(&self) -> bool {
        self.namespace.is_empty()
    }

    pub fn first_package(&self) -> Option<&str> {
        self.packages.first().map(String::as_str)
    }
}

/// A file that matches `exclude_from_prefix.file_patterns` and declares a
/// symbol, so is left as it is.
#[derive(Debug, Serialize)]
pub struct ExcludedFile {
    pub file: String,
    pub pattern: String,
}

#[derive(Debug, Default, Serialize)]
pub struct RenamePlan {
    /// The packages whose symbols are renamed: all the enumerated packages
    /// except those in `exclude_from_prefix.packages`.
    pub packages_to_prefix: Vec<String>,

    pub symbols: Vec<PlannedSymbol>,

    /// Files left as they are because of `exclude_from_prefix.file_patterns`.
    pub files_excluded_from_prefix: Vec<ExcludedFile>,

    /// Symbols that already have the prefix, such as a package prefixed in
    /// place on an earlier run and not reinstalled since. Code that still uses
    /// the original name is pointed at them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub already_prefixed: Vec<AlreadyPrefixed>,
}

/// A symbol whose declaration has the prefix already, and the name it had
/// before.
#[derive(Debug, Clone, Serialize)]
pub struct AlreadyPrefixed {
    pub kind: SymbolKind,
    /// The name without the prefix.
    pub original: String,
    /// The name as it is declared.
    pub current: String,
}

/// How many symbols of a kind there are, and how many are renamed.
#[derive(Debug, Default, Serialize)]
pub struct KindSummary {
    pub total: usize,
    pub renamed: usize,
}

/// A count of the plan.
#[derive(Debug, Serialize)]
pub struct PlanSummary {
    pub symbols: usize,
    pub renamed: usize,
    pub by_kind: BTreeMap<&'static str, KindSummary>,
    /// How many symbols there are for each kind of reason.
    pub reasons: BTreeMap<&'static str, usize>,
}

impl RenamePlan {
    pub fn summary(&self) -> PlanSummary {
        let mut by_kind: BTreeMap<&'static str, KindSummary> = BTreeMap::new();
        let mut reasons: BTreeMap<&'static str, usize> = BTreeMap::new();

        for symbol in &self.symbols {
            let kind = by_kind.entry(symbol.kind.as_str()).or_default();

            kind.total += 1;
            kind.renamed += usize::from(symbol.rename);

            *reasons.entry(symbol.reason.category()).or_default() += 1;
        }

        PlanSummary {
            symbols: self.symbols.len(),
            renamed: self.symbols.iter().filter(|symbol| symbol.rename).count(),
            by_kind,
            reasons,
        }
    }

    pub fn get(&self, kind: SymbolKind, name: &str) -> Option<&PlannedSymbol> {
        let name = name.trim_start_matches('\\');

        self.symbols.iter().find(|symbol| {
            symbol.kind == kind
                && match kind {
                    SymbolKind::Constant => symbol.name == name,
                    _ => symbol.name.eq_ignore_ascii_case(name),
                }
        })
    }

    pub fn renamed(&self) -> impl Iterator<Item = &PlannedSymbol> {
        self.symbols.iter().filter(|symbol| symbol.rename)
    }
}
