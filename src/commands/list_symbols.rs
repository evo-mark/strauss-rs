//! `list-symbols`: the symbols declared in some PHP files and directories, one
//! per line.
//!
//! This follows PHP Strauss' `list-symbols` (0.32.0). It needs no project:
//! every `.php` file under the paths is analysed, and the namespaces, classes,
//! interfaces, traits, enums, functions and constants they declare are listed,
//! sorted, grouped by type. Symbols PHP itself provides, which polyfills
//! redeclare, are left out, as they are when prefixing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::analyser;
use crate::symbols::analyse::analyse_files;
use crate::symbols::{BuiltIns, SymbolKind};

/// The types that can be listed, in the order they are printed.
pub const TYPES: [SymbolKind; 7] = [
    SymbolKind::Namespace,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Trait,
    SymbolKind::Enum,
    SymbolKind::Function,
    SymbolKind::Constant,
];

/// What was found.
#[derive(Debug, Default)]
pub struct Listing {
    /// The names of each type asked for, sorted, in the order of [`TYPES`].
    pub symbols: Vec<(SymbolKind, Vec<String>)>,

    /// Files that couldn't be analysed: (path, message).
    pub warnings: Vec<(String, String)>,
}

/// Which symbols to list.
#[derive(Debug, Clone)]
pub struct Filter {
    /// The types to list; empty for all of them.
    pub types: Vec<SymbolKind>,

    /// Only what is in the global namespace: what could clash with another
    /// plugin's copy. Namespaces are never global, so none are listed.
    pub global: bool,
}

impl Filter {
    /// The types to list, in the order of [`TYPES`].
    fn types(&self) -> Vec<SymbolKind> {
        TYPES
            .into_iter()
            .filter(|kind| self.types.is_empty() || self.types.contains(kind))
            .collect()
    }
}

/// Lists the symbols declared in the `.php` files under `paths`. A file named
/// directly is analysed whatever its extension.
pub fn list(paths: &[PathBuf], filter: &Filter, workers: usize) -> Result<Listing, String> {
    let files = php_files(paths)?;
    let file_refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();

    let workers = workers.min(files.len()).max(1);
    let analyses = analyse_files(&file_refs, workers);

    let types = filter.types();
    let built_ins = BuiltIns::get();

    let mut found: Vec<BTreeSet<String>> = vec![BTreeSet::new(); types.len()];

    let mut add = |kind: SymbolKind, name: &str| {
        if let Some(position) = types.iter().position(|wanted| *wanted == kind) {
            found[position].insert(name.to_string());
        }
    };

    let mut listing = Listing::default();

    for (path, analysis) in files.iter().zip(analyses) {
        let analysis = match analysis {
            Ok(analysed) => analysed.analysis,
            Err(message) => {
                listing.warnings.push((path.display().to_string(), message));

                continue;
            }
        };

        for namespace in &analysis.namespaces {
            if let Some(name) = namespace.namespace.as_ref().filter(|_| !filter.global) {
                add(SymbolKind::Namespace, name);
            }

            for declared in &namespace.symbols {
                let kind = kind_of(declared.kind);

                let is_built_in = match kind {
                    kind if kind.is_class_like() => built_ins.is_class_like(&declared.name),
                    SymbolKind::Function => built_ins.is_function(&declared.name),
                    _ => false,
                };

                if is_built_in || (filter.global && declared.name.contains('\\')) {
                    continue;
                }

                add(kind, &declared.name);
            }
        }
    }

    listing.symbols = types
        .into_iter()
        .zip(found)
        .map(|(kind, names)| (kind, names.into_iter().collect()))
        .collect();

    Ok(listing)
}

/// One symbol per line. With one type, just the names, for piping; otherwise
/// the type, then the name.
pub fn render(listing: &Listing) -> String {
    let only_names = listing.symbols.len() == 1;
    let mut lines: Vec<String> = Vec::new();

    for (kind, names) in &listing.symbols {
        for name in names {
            lines.push(if only_names {
                name.clone()
            } else {
                format!("{:<10} {name}", kind.as_str())
            });
        }
    }

    lines.join("\n")
}

fn kind_of(kind: analyser::SymbolKind) -> SymbolKind {
    match kind {
        analyser::SymbolKind::Class => SymbolKind::Class,
        analyser::SymbolKind::Interface => SymbolKind::Interface,
        analyser::SymbolKind::Trait => SymbolKind::Trait,
        analyser::SymbolKind::Enum => SymbolKind::Enum,
        analyser::SymbolKind::Function => SymbolKind::Function,
        analyser::SymbolKind::Constant => SymbolKind::Constant,
    }
}

/// The files to analyse, in a stable order. A directory is walked for `.php`
/// files, leaving out `.git` and what a `.gitignore` inside it ignores, as the
/// files of a package are listed.
fn php_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();

    for path in paths {
        if path.is_file() {
            files.insert(path.clone());

            continue;
        }

        if !path.is_dir() {
            return Err(format!("{} is not a file or directory", path.display()));
        }

        let mut builder = WalkBuilder::new(path);

        builder
            .hidden(false)
            .parents(false)
            .ignore(false)
            .git_global(false)
            .git_exclude(false)
            .require_git(false)
            .follow_links(false)
            .git_ignore(true)
            .filter_entry(|entry| !(entry.depth() > 0 && entry.file_name() == ".git"));

        for entry in builder.build() {
            let entry = entry.map_err(|error| error.to_string())?;

            if entry.file_type().is_some_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "php")
            {
                files.insert(entry.into_path());
            }
        }
    }

    Ok(files.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("strauss-rs-list-symbols")
            .join(test);

        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        dir
    }

    fn write(dir: &Path, path: &str, text: &str) {
        let path = dir.join(path);

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn project(test: &str) -> PathBuf {
        let dir = scratch(test);

        write(
            &dir,
            "src/Client.php",
            "<?php\nnamespace Acme\\Lib;\n\ninterface Sender {}\nclass Client implements Sender {}\nfunction helper() {}\nconst VERSION = 1;\n",
        );
        write(
            &dir,
            "global.php",
            "<?php\nclass Acme_Legacy {}\nfunction acme_helper() {}\ndefine('ACME_DEBUG', true);\nif (!function_exists('str_contains')) { function str_contains($a, $b) {} }\n",
        );
        write(&dir, "README.md", "class NotPhp {}");

        dir
    }

    fn all() -> Filter {
        Filter {
            types: Vec::new(),
            global: false,
        }
    }

    #[test]
    fn lists_every_type_in_order_with_the_type_first() {
        let dir = project("all");
        let listing = list(&[dir], &all(), 1).unwrap();

        assert_eq!(
            render(&listing),
            [
                "namespace  Acme\\Lib",
                "class      Acme\\Lib\\Client",
                "class      Acme_Legacy",
                "interface  Acme\\Lib\\Sender",
                "function   Acme\\Lib\\helper",
                "function   acme_helper",
                "constant   ACME_DEBUG",
                "constant   Acme\\Lib\\VERSION",
            ]
            .join("\n")
        );
    }

    #[test]
    fn one_type_is_just_the_names() {
        let dir = project("one-type");
        let filter = Filter {
            types: vec![SymbolKind::Function],
            global: false,
        };

        assert_eq!(
            render(&list(&[dir], &filter, 1).unwrap()),
            "Acme\\Lib\\helper\nacme_helper"
        );
    }

    #[test]
    fn global_leaves_out_namespaces_and_what_is_in_them() {
        let dir = project("global");
        let filter = Filter {
            types: Vec::new(),
            global: true,
        };

        assert_eq!(
            render(&list(&[dir], &filter, 1).unwrap()),
            [
                "class      Acme_Legacy",
                "function   acme_helper",
                "constant   ACME_DEBUG",
            ]
            .join("\n")
        );
    }

    #[test]
    fn several_paths_and_a_file_named_directly() {
        let dir = project("paths");

        write(&dir, "other/Thing.inc", "<?php\nclass Thing {}\n");

        let filter = Filter {
            types: vec![SymbolKind::Class],
            global: false,
        };

        let listing = list(&[dir.join("src"), dir.join("other/Thing.inc")], &filter, 1).unwrap();

        assert_eq!(render(&listing), "Acme\\Lib\\Client\nThing");
    }

    #[test]
    fn a_missing_path_is_an_error() {
        let dir = scratch("missing");

        assert!(list(&[dir.join("nope")], &all(), 1).is_err());
    }

    #[test]
    fn a_file_that_cannot_be_parsed_is_a_warning() {
        let dir = scratch("broken");

        write(&dir, "broken.php", "<?php\nclass {\n");
        write(&dir, "fine.php", "<?php\nclass Fine {}\n");

        let listing = list(&[dir], &all(), 1).unwrap();

        assert_eq!(listing.warnings.len(), 1);
        assert!(render(&listing).contains("Fine"));
    }
}
