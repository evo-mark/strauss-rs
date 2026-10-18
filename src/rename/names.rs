//! The new names for the symbols marked for renaming.
//!
//! This follows PHP Strauss' `ChangeEnumerator`. A namespace is rewritten by
//! the first `namespace_replacement_patterns` entry that changes it, else by
//! putting `namespace_prefix` in front. Everything in a namespace follows it.
//! What is in the global namespace has no namespace to follow, so gets a prefix
//! of its own: `classmap_prefix` for classes, interfaces, traits and enums,
//! `functions_prefix` for functions and `constant_prefix` for constants.

use std::collections::HashMap;

use crate::config::Config;
use crate::packages::PackageSet;
use crate::rename::autoload::package_relative_path;
use crate::rename::model::{AlreadyPrefixed, PlannedSymbol, Reason};
use crate::symbols::SymbolKind;

/// What happens to a namespace.
struct NamespaceOutcome {
    replacement: Option<String>,
}

pub(super) fn assign(config: &Config, packages: &PackageSet, entries: &mut [PlannedSymbol]) {
    let namespaces = namespace_replacements(config, entries);

    for index in 0..entries.len() {
        if entries[index].kind == SymbolKind::Namespace {
            continue;
        }

        // Functions the user wants left alone inside strings, as array keys.
        if entries[index].kind == SymbolKind::Function
            && entries[index].is_global()
            && config
                .exclude_from_string_rename
                .functions
                .iter()
                .any(|function| *function == entries[index].name)
        {
            entries[index].replace_in_strings = false;
        }

        if !entries[index].marked {
            continue;
        }

        let outcome = if entries[index].is_global() {
            global_replacement(config, packages, entries, &namespaces, index)
        } else {
            namespaced_replacement(&entries[index], &namespaces)
        };

        apply(&mut entries[index], outcome);
    }

    follow_namespaces(entries, &namespaces);
}

/// The symbols whose declarations already have the prefix, with the names they
/// had without it.
///
/// With the target the vendor directory, a package that isn't reinstalled by
/// `composer update` keeps the names it was given. Code that is installed
/// afterwards still uses the original ones, and has to be pointed at them. Only
/// what `namespace_prefix`, `classmap_prefix` and `functions_prefix` put on can
/// be taken off again; a name made by `namespace_replacement_patterns` can't.
pub(super) fn already_prefixed(config: &Config, entries: &[PlannedSymbol]) -> Vec<AlreadyPrefixed> {
    let exists = |kind: SymbolKind, name: &str| {
        entries.iter().any(|entry| {
            entry.kind == kind
                && entry
                    .name
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(name)
        })
    };

    let mut found = Vec::new();

    for entry in entries.iter().filter(|entry| !entry.rename) {
        let original = if entry.is_global() && entry.kind != SymbolKind::Namespace {
            let prefix = match entry.kind {
                SymbolKind::Function => config.functions_prefix.as_deref(),
                SymbolKind::Constant => None,
                _ => config.classmap_prefix.as_deref(),
            };

            prefix
                .filter(|prefix| !prefix.is_empty() && entry.reason == Reason::AlreadyPrefixed)
                .and_then(|prefix| entry.name.strip_prefix(prefix))
                .filter(|original| !original.is_empty())
        } else if entry.kind == SymbolKind::Constant {
            None
        } else {
            config
                .namespace_prefix
                .as_deref()
                .and_then(|prefix| strip_once(&entry.name, prefix))
        };

        if let Some(original) = original.filter(|original| !exists(entry.kind, original)) {
            found.push(AlreadyPrefixed {
                kind: entry.kind,
                original: original.to_string(),
                current: entry.name.clone(),
            });
        }
    }

    found
}

/// What is left of the name after one whole-segment `prefix`, if it has one.
fn strip_once<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = prefix.trim_matches('\\');

    name.strip_prefix(prefix)?
        .strip_prefix('\\')
        .filter(|rest| !rest.is_empty())
}

/// A symbol that isn't renamed for its own sake still moves if its namespace
/// does: the namespace is rewritten in every file that declares it, and the
/// symbol is in one of them. (PHP Strauss gets this from replacing a namespace
/// as text wherever it is.) Symbols left alone because of where they are
/// declared keep their reason; so does any whose namespace isn't renamed.
fn follow_namespaces(
    entries: &mut [PlannedSymbol],
    namespaces: &HashMap<String, NamespaceOutcome>,
) {
    for entry in entries.iter_mut() {
        let follows = !entry.rename
            && !entry.is_global()
            && entry.kind != SymbolKind::Namespace
            && matches!(
                entry.reason,
                Reason::NotAutoloaded
                    | Reason::ConstantExcluded
                    | Reason::ConstantNamespaceExcluded(_)
                    | Reason::ConstantPackageExcluded(_)
                    | Reason::ConstantFileExcluded { .. }
            );

        if !follows {
            continue;
        }

        if let Ok(replacement) = namespaced_replacement(entry, namespaces) {
            entry.replacement = Some(replacement);
            entry.rename = true;
            entry.reason = Reason::FollowsNamespace;
        }
    }
}

/// The result of working out a name: the new name, or why there isn't one.
type Outcome = Result<String, Reason>;

fn apply(entry: &mut PlannedSymbol, outcome: Outcome) {
    match outcome {
        Ok(replacement) if replacement != entry.name => {
            entry.replacement = Some(replacement);
            entry.rename = true;
        }
        Ok(_) => entry.reason = Reason::AlreadyPrefixed,
        Err(reason) => entry.reason = reason,
    }
}

/// The new name of each namespace marked for renaming, by lower-cased name.
fn namespace_replacements(
    config: &Config,
    entries: &mut [PlannedSymbol],
) -> HashMap<String, NamespaceOutcome> {
    let mut namespaces = HashMap::new();

    for entry in entries
        .iter_mut()
        .filter(|entry| entry.kind == SymbolKind::Namespace)
    {
        if !entry.marked {
            continue;
        }

        let outcome = namespace_replacement(config, &entry.name);

        let replacement = match &outcome {
            Ok(replacement) if *replacement != entry.name => Some(replacement.clone()),
            _ => None,
        };

        apply(entry, outcome);

        namespaces.insert(
            entry.name.to_ascii_lowercase(),
            NamespaceOutcome { replacement },
        );
    }

    namespaces
}

/// The first `namespace_replacement_patterns` entry that changes the
/// namespace, else `namespace_prefix` in front.
pub(super) fn namespace_replacement(config: &Config, namespace: &str) -> Outcome {
    for entry in &config.namespace_replacement_patterns {
        // A pattern that fails to run counts as not matching.
        let Ok(replaced) = entry.pattern.replace_all(namespace, &entry.replacement) else {
            continue;
        };

        if replaced != namespace {
            return Ok(replaced);
        }
    }

    let Some(prefix) = &config.namespace_prefix else {
        return Err(Reason::NoNamespaceReplacement);
    };

    // A namespace that already has the prefix, even repeated, doesn't get it
    // again.
    match strip_prefix(namespace, prefix) {
        "" => Ok(prefix.clone()),
        stripped => Ok(format!("{prefix}\\{stripped}")),
    }
}

/// Removes the prefix from the front of the namespace, as many times as it is
/// there. It has to be a whole namespace segment: `Acme` is not the prefix of
/// `AcmeCorp\Lib`.
pub(super) fn strip_prefix<'a>(namespace: &'a str, prefix: &str) -> &'a str {
    let mut rest = namespace;

    loop {
        let Some(after) = rest.strip_prefix(prefix) else {
            return rest;
        };

        if !after.is_empty() && !after.starts_with('\\') {
            return rest;
        }

        let after = after.trim_start_matches('\\');

        if after.len() == rest.len() {
            return rest;
        }

        rest = after;
    }
}

/// A symbol in a namespace follows the namespace.
fn namespaced_replacement(
    entry: &PlannedSymbol,
    namespaces: &HashMap<String, NamespaceOutcome>,
) -> Outcome {
    let not_renamed = || Reason::NamespaceNotRenamed(entry.namespace.clone());

    match namespaces
        .get(&entry.namespace.to_ascii_lowercase())
        .and_then(|namespace| namespace.replacement.as_ref())
    {
        Some(replacement) => Ok(format!("{replacement}\\{}", entry.local_name())),
        None => Err(not_renamed()),
    }
}

fn global_replacement(
    config: &Config,
    packages: &PackageSet,
    entries: &[PlannedSymbol],
    namespaces: &HashMap<String, NamespaceOutcome>,
    index: usize,
) -> Outcome {
    let entry = &entries[index];

    match entry.kind {
        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait | SymbolKind::Enum => {
            let prefix = config
                .classmap_prefix
                .as_deref()
                .ok_or(Reason::NoPrefixConfigured("classmap_prefix"))?;

            // Already prefixed, unless the name is just the prefix.
            if entry.name.starts_with(prefix) && entry.name != prefix {
                return Err(Reason::AlreadyPrefixed);
            }

            Ok(format!("{prefix}{}", entry.name))
        }

        SymbolKind::Function => {
            let prefix = config
                .functions_prefix
                .as_deref()
                .ok_or(Reason::NoPrefixConfigured("functions_prefix"))?;

            if entry.name.starts_with(prefix) {
                return Err(Reason::AlreadyPrefixed);
            }

            Ok(function_with_prefix(
                entry, prefix, packages, entries, namespaces,
            ))
        }

        SymbolKind::Constant => {
            let prefix = config
                .constant_prefix
                .as_deref()
                .ok_or(Reason::NoPrefixConfigured("constant_prefix"))?;

            Ok(format!("{prefix}{}", entry.name))
        }

        SymbolKind::Namespace => unreachable!("namespaces are not global"),
    }
}

/// A global function gets `functions_prefix`, unless it is in a package that
/// autoloads with PSR-0, where classes are named for their namespace with
/// underscores (`Acme_Lib_Thing`) and the function is named to match.
fn function_with_prefix(
    entry: &PlannedSymbol,
    prefix: &str,
    packages: &PackageSet,
    entries: &[PlannedSymbol],
    namespaces: &HashMap<String, NamespaceOutcome>,
) -> String {
    let plain = || format!("{prefix}{}", entry.name);

    let Some(psr0_namespace) = psr0_namespace(entry, packages) else {
        return plain();
    };

    let psr0_namespace = psr0_namespace.trim_matches('\\');

    let Some(namespace) = entries.iter().find(|candidate| {
        candidate.kind == SymbolKind::Namespace
            && candidate.name.eq_ignore_ascii_case(psr0_namespace)
    }) else {
        return plain();
    };

    let replacement = namespaces
        .get(&namespace.name.to_ascii_lowercase())
        .and_then(|outcome| outcome.replacement.clone())
        .unwrap_or_else(|| namespace.name.clone());

    let underscored_original = namespace.local_name().replace('\\', "_");
    let underscored_new = replacement.replace('\\', "_");

    let mut parts: Vec<&str> = entry.name.split('_').collect();
    parts.pop();

    // Nothing before the last underscore: not named for a namespace.
    if parts.join("_").is_empty() {
        return plain();
    }

    let unnamespaced = entry
        .name
        .strip_prefix(&underscored_original)
        .unwrap_or(&entry.name);

    format!("{underscored_new}{unnamespaced}")
        .trim_matches('_')
        .to_string()
}

/// The `psr-0` namespace key of a package that autoloads one of the files the
/// symbol is declared in.
fn psr0_namespace(entry: &PlannedSymbol, packages: &PackageSet) -> Option<String> {
    for name in &entry.packages {
        let Some(package) = packages.get(name) else {
            continue;
        };

        for source in entry
            .sources
            .iter()
            .filter(|source| source.package == *name)
        {
            let relative =
                package_relative_path(&source.package, &source.file).trim_matches(&['\\', '/'][..]);

            for (key, directories) in &package.autoload.psr0.0 {
                if directories
                    .iter()
                    .any(|directory| relative.starts_with(directory.trim_matches(&['\\', '/'][..])))
                {
                    return Some(key.clone());
                }
            }
        }
    }

    None
}
