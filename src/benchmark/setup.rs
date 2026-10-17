//! Getting what the benchmark needs: the Strauss PHP phar, and each project's
//! packages from the lock file.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::benchmark::Result;
use crate::benchmark::projects::{Manifest, Project};
use crate::output::composer_command;

pub fn setup(root: &Path, manifest: &Manifest, projects: &[Project]) -> Result {
    let phar = manifest.phar(root);

    if phar.is_file() {
        eprintln!(
            "Strauss PHP {} is already at {}",
            manifest.strauss_php.version,
            phar.display()
        );
    } else {
        fs::create_dir_all(root.join("tools"))?;

        eprintln!("Downloading Strauss PHP {}", manifest.strauss_php.version);

        // Not renamed into place until it is all there.
        let partial = phar.with_extension("phar.part");

        let status = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--output",
            ])
            .arg(&partial)
            .arg(&manifest.strauss_php.url)
            .status()
            .map_err(|error| format!("could not run curl: {error}"))?;

        if !status.success() {
            let _ = fs::remove_file(&partial);

            return Err(format!("could not download {}", manifest.strauss_php.url).into());
        }

        fs::rename(&partial, &phar)?;
    }

    let composer = composer_command();

    for project in projects {
        eprintln!("Installing the packages of {}", project.name);

        let status = Command::new(&composer)
            .args([
                "install",
                "--no-interaction",
                "--no-progress",
                "--no-scripts",
            ])
            .current_dir(&project.dir)
            .status()
            .map_err(|error| format!("could not run `{composer}`: {error}"))?;

        if !status.success() {
            return Err(format!("`{composer} install` failed in {}", project.dir.display()).into());
        }
    }

    eprintln!("Ready. Run: cargo run --release --features benchmark -- benchmark run");

    Ok(())
}
