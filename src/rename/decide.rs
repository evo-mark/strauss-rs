//! Which symbols are marked for renaming, before their new names are worked
//! out.
//!
//! A symbol is marked when it is autoloaded, or when its namespace matches a
//! `namespace_replacement_patterns` entry, unless an exclusion applies. This
//! follows PHP Strauss' `MarkSymbolsForRenaming`.

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::packages::{PackageFile, PackageSet, is_target_the_vendor_directory};
use crate::php_pattern::PhpPattern;
use crate::rename::is_within_namespace;
use crate::rename::model::{ExcludedFile, PlannedSymbol, Reason};
use crate::symbols::SymbolKind;

/// Files that match `exclude_from_prefix.file_patterns`, whether or not they
/// declare anything: they are left as they are.
pub(super) fn files_excluded_from_prefix(
    config: &Config,
    files: &[PackageFile],
) -> Vec<ExcludedFile> {
    files
        .iter()
        .filter_map(|file| {
            first_match(
                &config.exclude_from_prefix.file_patterns,
                &file.vendor_relative_path,
            )
            .map(|pattern| ExcludedFile {
                file: file.vendor_relative_path.clone(),
                pattern: pattern.source().to_string(),
            })
        })
        .collect()
}

pub(super) fn decide(
    config: &Config,
    packages: &PackageSet,
    files: &[PackageFile],
    excluded_files: &[ExcludedFile],
    entries: &mut [PlannedSymbol],
) {
    let copied: HashMap<&str, bool> = files
        .iter()
        .map(|file| (file.vendor_relative_path.as_str(), file.copy))
        .collect();

    let excluded: HashMap<&str, &ExcludedFile> = excluded_files
        .iter()
        .map(|file| (file.file.as_str(), file))
        .collect();

    let target_is_vendor = is_target_the_vendor_directory(config);

    let unprefixed: HashSet<String> = packages
        .packages
        .iter()
        .filter(|package| package.unprefixed)
        .map(|package| package.name.to_ascii_lowercase())
        .collect();

    for entry in entries.iter_mut() {
        let namespace = match entry.kind {
            SymbolKind::Namespace => entry.name.clone(),
            _ if entry.is_global() => "\\".to_string(),
            _ => entry.namespace.clone(),
        };

        let matches_pattern = config
            .namespace_replacement_patterns
            .iter()
            .any(|replacement| replacement.pattern.is_match(&namespace));

        entry.marked = entry.autoloaded || matches_pattern;
        entry.reason = if entry.autoloaded {
            Reason::Autoloaded
        } else if matches_pattern {
            Reason::MatchesNamespaceReplacementPattern
        } else {
            Reason::NotAutoloaded
        };

        let reason = not_prefixed(entry, &unprefixed)
            .or_else(|| exclusion(config, entry, &copied, &excluded, target_is_vendor));

        let Some(reason) = reason else {
            continue;
        };

        entry.marked = false;
        entry.reason = reason;
    }
}

/// Whether only packages outside the dependency tree of `packages` declare the
/// symbol. One that a prefixed package declares too is renamed: the prefixed
/// copy has the new name, and the other declaration is left as it is.
fn not_prefixed(entry: &PlannedSymbol, unprefixed: &HashSet<String>) -> Option<Reason> {
    let first = entry.first_package()?;

    entry
        .packages
        .iter()
        .all(|package| unprefixed.contains(&package.to_ascii_lowercase()))
        .then(|| Reason::PackageNotPrefixed(first.to_string()))
}

/// Why the symbol is excluded from renaming, if it is.
fn exclusion(
    config: &Config,
    entry: &PlannedSymbol,
    copied: &HashMap<&str, bool>,
    excluded_files: &HashMap<&str, &ExcludedFile>,
    target_is_vendor: bool,
) -> Option<Reason> {
    // The package that declares it first stands for the symbol, as in PHP
    // Strauss.
    let package = entry.first_package();

    if let Some(package) =
        package.filter(|package| contains(&config.exclude_from_copy.packages, package))
    {
        return Some(Reason::PackageExcludedFromCopy(package.to_string()));
    }

    if let Some(reason) = excluded_from_prefix(config, entry, package, excluded_files) {
        return Some(reason);
    }

    if entry.kind == SymbolKind::Constant {
        if let Some(reason) = excluded_constant(config, entry, package) {
            return Some(reason);
        }
    }

    // A symbol only declared in files that aren't copied would be renamed
    // everywhere it's used but not where it's declared. (PHP Strauss intends
    // this too, but its check is overwritten by the line after it.)
    if !target_is_vendor
        && entry.kind != SymbolKind::Namespace
        && !entry
            .sources
            .iter()
            .any(|source| copied.get(source.file.as_str()).copied().unwrap_or(false))
    {
        return Some(Reason::OnlyInFilesNotCopied);
    }

    None
}

fn excluded_from_prefix(
    config: &Config,
    entry: &PlannedSymbol,
    package: Option<&str>,
    excluded_files: &HashMap<&str, &ExcludedFile>,
) -> Option<Reason> {
    let exclusions = &config.exclude_from_prefix;

    if let Some(package) = package.filter(|package| contains(&exclusions.packages, package)) {
        return Some(Reason::PackageExcludedFromPrefix(package.to_string()));
    }

    if entry.kind == SymbolKind::Function {
        if let Some(function) = exclusions
            .functions
            .iter()
            .find(|function| function.eq_ignore_ascii_case(&entry.name))
        {
            return Some(Reason::FunctionExcludedFromPrefix(function.clone()));
        }
    }

    // Excluded when every file that declares it is. A namespace several files
    // share is still renamed for the others, though PHP Strauss would leave it
    // as it is because of any one of them.
    if !entry.sources.is_empty()
        && entry
            .sources
            .iter()
            .all(|source| excluded_files.contains_key(source.file.as_str()))
    {
        let first = excluded_files[entry.sources[0].file.as_str()];

        return Some(Reason::FileExcludedFromPrefix {
            pattern: first.pattern.clone(),
            file: first.file.clone(),
        });
    }

    let namespace = match entry.kind {
        SymbolKind::Namespace => entry.name.as_str(),
        _ => entry.namespace.as_str(),
    };

    within_any(&exclusions.namespaces, namespace)
        .map(|excluded| Reason::NamespaceExcludedFromPrefix(excluded.to_string()))
}

fn excluded_constant(
    config: &Config,
    entry: &PlannedSymbol,
    package: Option<&str>,
) -> Option<Reason> {
    let exclusions = &config.exclude_constants;

    if let Some(package) =
        package.filter(|package| contains(&exclusions.exclusions.packages, package))
    {
        return Some(Reason::ConstantPackageExcluded(package.to_string()));
    }

    if let Some(excluded) = within_any(&exclusions.exclusions.namespaces, &entry.namespace) {
        return Some(Reason::ConstantNamespaceExcluded(excluded.to_string()));
    }

    for source in &entry.sources {
        if let Some(pattern) = first_match(&exclusions.exclusions.file_patterns, &source.file) {
            return Some(Reason::ConstantFileExcluded {
                pattern: pattern.source().to_string(),
                file: source.file.clone(),
            });
        }
    }

    if exclusions
        .constants
        .iter()
        .any(|constant| *constant == entry.name)
    {
        return Some(Reason::ConstantExcluded);
    }

    None
}

/// Package names are case-insensitive.
fn contains(packages: &[String], package: &str) -> bool {
    packages
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(package))
}

fn first_match<'a>(patterns: &'a [PhpPattern], path: &str) -> Option<&'a PhpPattern> {
    patterns.iter().find(|pattern| pattern.is_match(path))
}

/// The entry that `namespace` is, or is within. The global namespace is within
/// nothing.
fn within_any<'a>(excluded: &'a [String], namespace: &str) -> Option<&'a str> {
    if namespace.is_empty() {
        return None;
    }

    excluded
        .iter()
        .map(String::as_str)
        .find(|excluded| is_within_namespace(namespace, excluded))
}
