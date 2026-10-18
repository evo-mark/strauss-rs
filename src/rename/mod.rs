//! Deciding which symbols are renamed, and to what.
//!
//! The steps follow PHP Strauss:
//!
//! 1. The namespaces in a package's autoload rules are added ([`add_autoload_namespaces`]).
//! 2. Autoloaded symbols are found (`autoload`).
//! 3. Symbols are marked for renaming, minus exclusions (`decide`).
//! 4. New names are worked out (`names`).
//!
//! Before that, [`exclude_namespaces_from_copy`] finishes deciding which files
//! are copied, which needs to know the namespaces they declare.

#![allow(dead_code)]

mod autoload;
mod decide;
mod model;
mod names;

// The module's API; the CLI uses some of it so far.
#[allow(unused_imports)]
pub use model::*;

use crate::config::Config;
use crate::packages::{CopyExclusion, PackageFile, PackageSet};
use crate::symbols::{SymbolKind, SymbolTable};

/// Whether `namespace` is `ancestor` or inside it: `Psr\Log\Foo` is within
/// `Psr\Log`, `Psr\LogExtra` is not. A leading or trailing backslash on
/// `ancestor` is ignored.
///
/// PHP Strauss compares the raw start of the strings, so excluding `Psr\Log`
/// would also exclude `Psr\LogExtra`.
pub(crate) fn is_within_namespace(namespace: &str, ancestor: &str) -> bool {
    let namespace = namespace.trim_matches('\\');
    let ancestor = ancestor.trim_matches('\\');

    match namespace.strip_prefix(ancestor) {
        Some("") => true,
        Some(rest) => rest.starts_with('\\'),
        None => false,
    }
}

/// Stops files that declare a namespace in `exclude_from_copy.namespaces` being
/// copied.
///
/// This is the part of deciding what to copy that needs the files' symbols; the
/// rest is done when the files are listed. A file already excluded keeps its
/// first reason.
pub fn exclude_namespaces_from_copy(
    config: &Config,
    files: &mut [PackageFile],
    table: &SymbolTable,
) {
    if config.exclude_from_copy.namespaces.is_empty() {
        return;
    }

    let index: std::collections::HashMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.vendor_relative_path.as_str(), index))
        .collect();

    let mut excluded: Vec<(usize, String)> = Vec::new();

    for scanned in &table.files {
        let (Some(analysis), Some(&position)) = (
            &scanned.analysis,
            index.get(scanned.vendor_relative_path.as_str()),
        ) else {
            continue;
        };

        let declared = analysis
            .namespaces
            .iter()
            .filter_map(|namespace| namespace.namespace.as_deref());

        for namespace in declared {
            if let Some(entry) = config
                .exclude_from_copy
                .namespaces
                .iter()
                .find(|entry| is_within_namespace(namespace, entry))
            {
                excluded.push((position, entry.clone()));

                break;
            }
        }
    }

    for (position, entry) in excluded {
        if files[position].copy {
            files[position].copy = false;
            files[position].not_copied_because = Some(CopyExclusion::NamespaceExcluded(entry));
        }
    }
}

/// Works out which symbols are renamed, and what to.
///
/// `files` is the result of [`exclude_namespaces_from_copy`].
pub fn plan(
    config: &Config,
    packages: &PackageSet,
    files: &[PackageFile],
    table: &SymbolTable,
) -> RenamePlan {
    let packages_to_prefix: Vec<String> = packages
        .packages
        .iter()
        .filter(|package| {
            !config
                .exclude_from_prefix
                .packages
                .iter()
                .any(|excluded| excluded.eq_ignore_ascii_case(&package.name))
        })
        .map(|package| package.name.clone())
        .collect();

    let mut entries: Vec<PlannedSymbol> = table
        .symbols
        .iter()
        .map(|symbol| PlannedSymbol {
            kind: symbol.kind,
            name: symbol.name.clone(),
            namespace: symbol.namespace.clone(),
            packages: symbol.packages().into_iter().map(String::from).collect(),
            sources: symbol.sources.clone(),
            autoloaded: false,
            rename: false,
            replacement: None,
            reason: Reason::NotAutoloaded,
            replace_in_strings: true,
            psr: None,
            marked: false,
        })
        .collect();

    add_autoload_namespaces(&mut entries, packages, &packages_to_prefix);
    autoload::mark(&mut entries, table, packages);

    let files_excluded_from_prefix = decide::files_excluded_from_prefix(config, files);

    decide::decide(config, files, &files_excluded_from_prefix, &mut entries);
    names::assign(config, packages, &mut entries);

    let already_prefixed = names::already_prefixed(config, &entries);

    RenamePlan {
        packages_to_prefix,
        symbols: entries,
        files_excluded_from_prefix,
        already_prefixed,
    }
}

/// Adds the namespace of every `psr-0` and `psr-4` rule of the packages being
/// prefixed, even if no file declares it: other code refers to it through the
/// autoloader.
fn add_autoload_namespaces(
    entries: &mut Vec<PlannedSymbol>,
    packages: &PackageSet,
    packages_to_prefix: &[String],
) {
    for package_name in packages_to_prefix {
        let Some(package) = packages.get(package_name) else {
            continue;
        };

        let rules = package
            .autoload
            .psr4
            .0
            .iter()
            .map(|(key, _)| (key, Psr::Psr4))
            .chain(
                package
                    .autoload
                    .psr0
                    .0
                    .iter()
                    .map(|(key, _)| (key, Psr::Psr0)),
            );

        for (key, psr) in rules {
            let name = key.trim_matches('\\');

            if name.is_empty() {
                continue;
            }

            let existing = entries.iter_mut().find(|entry| {
                entry.kind == SymbolKind::Namespace && entry.name.eq_ignore_ascii_case(name)
            });

            match existing {
                Some(entry) => {
                    entry.autoloaded = true;
                    entry.psr.get_or_insert(psr);

                    if !entry.packages.contains(&package.name) {
                        entry.packages.push(package.name.clone());
                    }
                }
                None => entries.push(PlannedSymbol {
                    kind: SymbolKind::Namespace,
                    name: name.to_string(),
                    namespace: name.to_string(),
                    packages: vec![package.name.clone()],
                    sources: Vec::new(),
                    autoloaded: true,
                    rename: false,
                    replacement: None,
                    reason: Reason::Autoloaded,
                    replace_in_strings: true,
                    psr: Some(psr),
                    marked: false,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests;
