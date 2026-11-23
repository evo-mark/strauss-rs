//! Working out which packages to process: the configured packages, and
//! everything they require.
//!
//! This follows PHP Strauss' `DependenciesEnumerator`, with one difference in
//! where it looks. Strauss reads each package's `composer.json` from
//! `vendor/<name>`, falling back to `composer.lock`. This reads
//! `vendor/composer/installed.json`, which Composer writes with the same
//! metadata and, unlike the vendor layout, says where each package really is.
//! A `wordpress-plugin` package, for example, is installed under `plugins/`,
//! not `vendor/`. The package's own `composer.json` is the fallback for a
//! package `installed.json` doesn't list.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::config::Config;
use crate::packages::files::is_target_the_vendor_directory;
use crate::packages::installed::{InstalledIndex, InstalledPackage};
use crate::packages::model::{Package, PackageSet, PackageWarning, SkipReason, SkippedPackage};

/// Names that mean "some implementation of this interface" rather than a
/// package. PHP Strauss never tries to find these.
const VIRTUAL_PACKAGES: &[&str] = &["php-http/client-implementation"];

/// `php`, `ext-json`, `lib-curl`, `composer-runtime-api`…: requirements on the
/// environment, not packages.
///
/// PHP Strauss treats anything starting with `ext` as one, including a real
/// package called `extra/thing`; this follows Composer's own definition.
pub fn is_platform_package(name: &str) -> bool {
    let name = name.to_ascii_lowercase();

    match name.as_str() {
        "php"
        | "php-64bit"
        | "php-ipv6"
        | "php-zts"
        | "php-debug"
        | "hhvm"
        | "composer"
        | "composer-plugin-api"
        | "composer-runtime-api" => true,
        _ => ["ext-", "lib-"]
            .iter()
            .filter_map(|prefix| name.strip_prefix(prefix))
            .any(is_platform_suffix),
    }
}

fn is_platform_suffix(suffix: &str) -> bool {
    let mut characters = suffix.chars();

    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        })
}

pub fn resolve(config: &Config, index: &InstalledIndex) -> PackageSet {
    let mut resolver = Resolver {
        config,
        index,
        vendor_dir: config.project_dir.join(&config.vendor_directory),
        seen: HashSet::new(),
        set: PackageSet::default(),
        unprefixed: false,
    };

    for name in &config.packages {
        resolver.visit(name);
    }

    // Prefixing in place, the project's other non-dev packages stay in the
    // vendor directory beside the prefixed ones and have to use their new
    // names, as PHP Strauss does since 0.32.0. Packages outside the project's
    // `require` tree are dev packages, and are left alone.
    if is_target_the_vendor_directory(config) {
        resolver.unprefixed = true;

        for name in &config.project_requires {
            resolver.visit(name);
        }
    }

    link_dependencies(&mut resolver.set.packages);

    resolver.set
}

struct Resolver<'a> {
    config: &'a Config,
    index: &'a InstalledIndex,
    vendor_dir: PathBuf,
    /// Lower-cased names already visited, found or not.
    seen: HashSet<String>,
    set: PackageSet,
    /// Whether the packages visited now are outside the dependency tree of
    /// `packages`: [`Package::unprefixed`].
    unprefixed: bool,
}

impl Resolver<'_> {
    /// Adds the package, then its requirements, depth first: the order PHP
    /// Strauss visits them in.
    fn visit(&mut self, name: &str) {
        if is_platform_package(name)
            || VIRTUAL_PACKAGES
                .iter()
                .any(|virtual_name| virtual_name.eq_ignore_ascii_case(name))
        {
            return;
        }

        if !self.seen.insert(name.to_ascii_lowercase()) {
            return;
        }

        let Some(installed) = self.find(name) else {
            // Outside the tree, a missing package is no loss: nothing would be
            // prefixed in it.
            if self.unprefixed {
                return;
            }

            // A virtual package (`psr/log-implementation`) is satisfied by
            // whatever provides it, so it isn't missing.
            if !self.index.is_provided(name) {
                self.set.skipped.push(SkippedPackage {
                    name: name.to_string(),
                    reason: SkipReason::NotInstalled,
                });
            }

            return;
        };

        let package = self.build(&installed);
        let requires = package.requires.clone();

        self.set.packages.push(package);

        for required in requires {
            self.visit(&required);
        }
    }

    /// The package from `installed.json`, or failing that its own
    /// `composer.json` in the vendor directory.
    fn find(&self, name: &str) -> Option<InstalledPackage> {
        if let Some(installed) = self.index.get(name) {
            return Some(installed.clone());
        }

        let json = fs::read_to_string(self.vendor_dir.join(name).join("composer.json")).ok()?;
        let mut value: serde_json::Value = serde_json::from_str(&json).ok()?;

        // Not every package's composer.json says what it's called.
        value
            .as_object_mut()?
            .entry("name")
            .or_insert_with(|| serde_json::Value::String(name.to_string()));

        serde_json::from_value(value).ok()
    }

    fn build(&mut self, installed: &InstalledPackage) -> Package {
        let path = self.locate(installed);

        let overridden = self
            .config
            .override_autoload
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&installed.name))
            .map(|(_, autoload)| autoload.clone());

        Package {
            name: installed.name.clone(),
            version: installed.version.clone(),
            package_type: installed.package_type.clone(),
            path,
            autoload_overridden: overridden.is_some(),
            autoload: overridden.unwrap_or_else(|| installed.autoload.clone()),
            requires: installed
                .require
                .0
                .iter()
                .map(|(name, _)| name.clone())
                .filter(|name| !is_platform_package(name))
                .collect(),
            provides: installed.provided_names().map(String::from).collect(),
            suggests: installed
                .suggest
                .0
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
            license: installed
                .license
                .as_ref()
                .map(|license| license.iter().map(String::from).collect())
                .unwrap_or_default(),
            dependencies: Vec::new(),
            unprefixed: self.unprefixed,
        }
    }

    /// Where the package's files are.
    fn locate(&mut self, installed: &InstalledPackage) -> Option<PathBuf> {
        // A metapackage only lists requirements.
        if installed.package_type.as_deref() == Some("metapackage") {
            return None;
        }

        let expected = normalise(&match &installed.install_path {
            Some(install_path) => self.vendor_dir.join("composer").join(install_path),
            None => self.vendor_dir.join(&installed.name),
        });

        match fs::canonicalize(&expected) {
            Ok(path) if path.is_dir() => Some(path),
            _ => {
                self.set.warnings.push(PackageWarning {
                    package: installed.name.clone(),
                    message: format!("{} is not a directory", expected.display()),
                });

                None
            }
        }
    }
}

/// Resolves `.` and `..` without touching the filesystem, so a path reads as
/// `vendor/acme/lib` rather than `vendor/composer/../acme/lib`.
fn normalise(path: &Path) -> PathBuf {
    let mut normalised = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalised.pop();
            }
            other => normalised.push(other),
        }
    }

    normalised
}

/// Records, for each package, which of the others it depends on. A
/// requirement is met by the package of that name or, when there isn't one,
/// by a package that `provide`s or `replace`s the name.
fn link_dependencies(packages: &mut [Package]) {
    let mut providers: HashMap<String, Vec<usize>> = HashMap::new();

    for (index, package) in packages.iter().enumerate() {
        for provided in &package.provides {
            providers
                .entry(provided.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
    }

    let by_name: HashMap<String, usize> = packages
        .iter()
        .enumerate()
        .map(|(index, package)| (package.name.to_ascii_lowercase(), index))
        .collect();

    let dependencies: Vec<Vec<String>> = packages
        .iter()
        .enumerate()
        .map(|(index, package)| {
            let mut dependencies: Vec<String> = Vec::new();

            for required in &package.requires {
                let key = required.to_ascii_lowercase();

                let found: Vec<usize> = match by_name.get(&key) {
                    Some(found) => vec![*found],
                    None => providers.get(&key).cloned().unwrap_or_default(),
                };

                for found in found {
                    let name = &packages[found].name;

                    if found != index && !dependencies.contains(name) {
                        dependencies.push(name.clone());
                    }
                }
            }

            dependencies
        })
        .collect();

    for (package, dependencies) in packages.iter_mut().zip(dependencies) {
        package.dependencies = dependencies;
    }
}
