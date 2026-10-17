//! The projects that are prefixed, and the manifest that lists them.
//!
//! `bench/benchmark.json` names the projects and the Strauss PHP release to
//! compare against. Each project in `bench/projects/<name>` has a locked
//! `composer.json`, with the settings for Strauss PHP in `extra.strauss`, and a
//! `strauss.json` for Strauss-RS. The two have to ask for the same work, so
//! they are compared before anything runs.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub strauss_php: StraussPhp,

    /// The projects, smallest first.
    pub projects: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StraussPhp {
    pub version: String,
    pub url: String,
}

impl Manifest {
    pub fn load(root: &Path) -> Result<Manifest, String> {
        let path = root.join("benchmark.json");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;

        serde_json::from_str(&text)
            .map_err(|error| format!("{} is not valid: {error}", path.display()))
    }

    /// Where the Strauss PHP phar is kept.
    pub fn phar(&self, root: &Path) -> PathBuf {
        root.join("tools")
            .join(format!("strauss-{}.phar", self.strauss_php.version))
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub dir: PathBuf,

    /// Where the tools write, relative to the project.
    pub target_directory: String,
}

impl Project {
    pub fn new(root: &Path, name: &str) -> Result<Project, String> {
        let dir = root.join("projects").join(name);

        if !dir.join("composer.json").is_file() {
            return Err(format!("there is no project at {}", dir.display()));
        }

        let config = read_json(&dir.join("strauss.json"))?;

        Ok(Project {
            name: name.to_string(),
            target_directory: config
                .get("target_directory")
                .and_then(Value::as_str)
                .unwrap_or("vendor-prefixed")
                .trim_matches('/')
                .to_string(),
            dir,
        })
    }

    pub fn is_installed(&self) -> bool {
        self.dir.join("vendor/composer/installed.json").is_file()
    }

    /// The directory the tools write to.
    pub fn output(&self) -> PathBuf {
        self.dir.join(&self.target_directory)
    }
}

/// The projects to run, in the order of the manifest, or just those named.
pub fn select(root: &Path, manifest: &Manifest, only: &[String]) -> Result<Vec<Project>, String> {
    for name in only {
        if !manifest.projects.contains(name) {
            return Err(format!(
                "`{name}` is not a project in benchmark.json (it has: {})",
                manifest.projects.join(", ")
            ));
        }
    }

    manifest
        .projects
        .iter()
        .filter(|name| only.is_empty() || only.contains(name))
        .map(|name| Project::new(root, name))
        .collect()
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    serde_json::from_str(&text).map_err(|error| format!("{} is not valid: {error}", path.display()))
}

/// Where the settings for the two tools differ. If they do, one tool is given
/// different work, and the timings mean nothing.
pub fn config_problems(project: &Project) -> Vec<String> {
    let composer = match read_json(&project.dir.join("composer.json")) {
        Ok(composer) => composer,
        Err(message) => return vec![message],
    };

    let strauss = match read_json(&project.dir.join("strauss.json")) {
        Ok(strauss) => strauss,
        Err(message) => return vec![message],
    };

    let php = composer
        .pointer("/extra/strauss")
        .cloned()
        .unwrap_or(Value::Null);

    let mut problems = Vec::new();

    if php.is_null() {
        problems.push("composer.json has no extra.strauss".to_string());

        return problems;
    }

    let text = |value: &Value, key: &str, default: &str| -> String {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    };

    // Strauss PHP wants the trailing backslash and Strauss-RS doesn't mind it.
    let namespace = |value: &Value| {
        text(value, "namespace_prefix", "")
            .trim_end_matches('\\')
            .to_string()
    };

    if namespace(&php) != namespace(&strauss) {
        problems.push(format!(
            "namespace_prefix differs: `{}` and `{}`",
            namespace(&php),
            namespace(&strauss)
        ));
    }

    for (key, default) in [
        ("classmap_prefix", ""),
        ("target_directory", "vendor-prefixed"),
    ] {
        let (a, b) = (text(&php, key, default), text(&strauss, key, default));

        if a != b {
            problems.push(format!("{key} differs: `{a}` and `{b}`"));
        }
    }

    let packages = |value: &Value| -> Vec<String> {
        let mut names: Vec<String> = value
            .get("packages")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_ascii_lowercase)
                    .collect()
            })
            .unwrap_or_default();

        names.sort();

        names
    };

    if packages(&php) != packages(&strauss) {
        problems.push(format!(
            "packages differ: {:?} and {:?}",
            packages(&php),
            packages(&strauss)
        ));
    }

    // Anything else set on one side only is work the other doesn't do.
    for (side, value, other) in [
        ("composer.json", &php, &strauss),
        ("strauss.json", &strauss, &php),
    ] {
        if let Some(keys) = value.as_object() {
            for key in keys.keys() {
                if other.get(key).is_none() {
                    problems.push(format!("{key} is set in {side} only"));
                }
            }
        }
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bench_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bench")
    }

    #[test]
    fn the_manifest_lists_projects_that_exist() {
        let manifest = Manifest::load(&bench_root()).unwrap();

        assert!(!manifest.projects.is_empty());
        assert!(!manifest.strauss_php.version.is_empty());

        for name in &manifest.projects {
            Project::new(&bench_root(), name).unwrap();
        }
    }

    #[test]
    fn both_tools_are_given_the_same_work_in_every_project() {
        let manifest = Manifest::load(&bench_root()).unwrap();

        for name in &manifest.projects {
            let project = Project::new(&bench_root(), name).unwrap();

            assert_eq!(
                config_problems(&project),
                Vec::<String>::new(),
                "project {name}"
            );
        }
    }

    #[test]
    fn every_project_is_locked() {
        let manifest = Manifest::load(&bench_root()).unwrap();

        for name in &manifest.projects {
            assert!(
                bench_root()
                    .join("projects")
                    .join(name)
                    .join("composer.lock")
                    .is_file(),
                "project {name} has no composer.lock"
            );
        }
    }

    #[test]
    fn an_unknown_project_is_refused() {
        let manifest = Manifest::load(&bench_root()).unwrap();
        let error = select(&bench_root(), &manifest, &["nope".to_string()]).unwrap_err();

        assert!(error.contains("`nope` is not a project"), "{error}");
    }

    #[test]
    fn projects_are_chosen_in_the_order_of_the_manifest() {
        let manifest = Manifest::load(&bench_root()).unwrap();
        let chosen = select(
            &bench_root(),
            &manifest,
            &["medium".to_string(), "small".to_string()],
        )
        .unwrap();

        let names: Vec<&str> = chosen.iter().map(|p| p.name.as_str()).collect();

        assert_eq!(names, ["small", "medium"]);
    }

    #[test]
    fn settings_that_differ_between_the_tools_are_found() {
        let dir = std::env::temp_dir().join(format!("strauss-rs-bench-{}", std::process::id()));

        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("composer.json"),
            r#"{"extra":{"strauss":{"namespace_prefix":"A\\B\\","classmap_prefix":"A_","packages":["x/y"],"update_call_sites":true}}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("strauss.json"),
            r#"{"namespace_prefix":"A\\C","classmap_prefix":"A_","packages":["x/z"]}"#,
        )
        .unwrap();

        let project = Project {
            name: "t".to_string(),
            dir: dir.clone(),
            target_directory: "vendor-prefixed".to_string(),
        };

        let problems = config_problems(&project);

        fs::remove_dir_all(&dir).unwrap();

        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("namespace_prefix differs")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.starts_with("packages differ")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p == "update_call_sites is set in composer.json only"),
            "{problems:?}"
        );
    }
}
