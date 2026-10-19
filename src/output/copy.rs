//! Copying the files to the target directory, with their changes made.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use rayon::prelude::*;

use crate::edits::{FileEdits, SkipReason, apply_edits};
use crate::output::error::OutputError;
use crate::output::header::{Header, add_header};
use crate::output::{Input, Options, project_author};
use crate::packages::{Package, PackageFile, is_target_the_vendor_directory};
use crate::parallel;

#[derive(Debug, Default)]
pub(super) struct Copied {
    /// The files written, by `vendor_relative_path`.
    pub written: HashSet<String>,

    /// How many of them were changed.
    pub modified: usize,

    /// Files written, files changed and edits made, by package.
    pub packages: BTreeMap<String, PackageCounts>,

    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct PackageCounts {
    pub written: usize,
    pub modified: usize,
    pub edits: usize,
}

struct Job<'a> {
    file: &'a PackageFile,
    edits: Option<&'a FileEdits>,
}

/// Writes every file that is copied, changed where there are edits for it. When
/// the target is the vendor directory nothing is copied; the files with edits
/// are changed where they are.
pub(super) fn run(
    input: &Input,
    options: &Options,
    symlinked: &HashSet<String>,
) -> Result<Copied, OutputError> {
    let in_place = is_target_the_vendor_directory(input.config);
    let mut copied = Copied::default();

    let edits: HashMap<&str, &FileEdits> = input
        .edits
        .files
        .iter()
        .map(|file| (file.file.as_str(), file))
        .collect();

    let mut jobs = Vec::new();

    for file in input.files {
        let edits = edits.get(file.vendor_relative_path.as_str()).copied();

        if in_place {
            // Only what changes, and not in a package that is a link to somewhere else.
            if edits.is_none() {
                continue;
            }

            if symlinked.contains(&file.package.to_ascii_lowercase()) {
                copied.warnings.push(format!(
                    "{} is not changed: {} is a link to another directory",
                    file.vendor_relative_path, file.package
                ));
                continue;
            }
        } else if !file.copy {
            continue;
        }

        jobs.push(Job { file, edits });
    }

    let author = options
        .include_author
        .then(|| project_author(input.config))
        .flatten();

    let licenses: HashMap<String, &[String]> = input
        .packages
        .packages
        .iter()
        .map(|package: &Package| {
            (
                package.name.to_ascii_lowercase(),
                package.license.as_slice(),
            )
        })
        .collect();

    let workers = parallel::worker_count(input.config.parallel, jobs.len());

    let stage = crate::progress::stage("Writing", jobs.len());

    let results: Vec<Result<bool, OutputError>> = parallel::pool(workers).install(|| {
        jobs.par_iter()
            .map(|job| {
                let header = Header {
                    licenses: licenses
                        .get(&job.file.package.to_ascii_lowercase())
                        .copied()
                        .unwrap_or(&[]),
                    author: author.as_deref(),
                    date: options
                        .include_modified_date
                        .then_some(options.date.as_str()),
                };

                let result = write_file(job, &header, options.dry_run, in_place);

                stage.tick();

                result
            })
            .collect()
    });

    for (job, result) in jobs.iter().zip(results) {
        let counts = copied.packages.entry(job.file.package.clone()).or_default();

        counts.written += 1;

        if result? {
            copied.modified += 1;
            counts.modified += 1;
            counts.edits += job.edits.map_or(0, |edits| edits.edits.len());
        }

        copied.written.insert(job.file.vendor_relative_path.clone());
    }

    // Files that were copied as they were because they couldn't be rewritten.
    if !in_place {
        for skipped in &input.edits.skipped {
            let message = match skipped.reason {
                SkipReason::NotUtf8 | SkipReason::NotAnalysed
                    if copied.written.contains(&skipped.file) =>
                {
                    format!(
                        "{} was copied as it is: {}, so its names could not be updated",
                        skipped.file, skipped.reason
                    )
                }
                _ => continue,
            };

            copied.warnings.push(message);
        }
    }

    Ok(copied)
}

/// Whether the file was changed.
fn write_file(
    job: &Job,
    header: &Header,
    dry_run: bool,
    in_place: bool,
) -> Result<bool, OutputError> {
    let source = &job.file.source_path;
    let target = if in_place {
        source
    } else {
        &job.file.target_path
    };

    let Some(edits) = job.edits else {
        if !dry_run {
            create_parent(target)?;
            fs::copy(source, target).map_err(|error| OutputError::io("copy", target, error))?;
        }

        return Ok(false);
    };

    if dry_run {
        return Ok(true);
    }

    let original =
        fs::read_to_string(source).map_err(|error| OutputError::io("read", source, error))?;

    let mut changed = apply_edits(&original, &edits.edits).map_err(|error| OutputError::Edit {
        file: job.file.vendor_relative_path.clone(),
        message: error.to_string(),
    })?;

    changed = add_header(&changed, header);

    create_parent(target)?;
    fs::write(target, changed).map_err(|error| OutputError::io("write", target, error))?;

    // Keep the file's mode, so an executable stays one.
    if let Ok(metadata) = fs::metadata(source) {
        let _ = fs::set_permissions(target, metadata.permissions());
    }

    Ok(true)
}

fn create_parent(path: &Path) -> Result<(), OutputError> {
    match path.parent() {
        Some(parent) => {
            fs::create_dir_all(parent).map_err(|error| OutputError::io("create", parent, error))
        }
        None => Ok(()),
    }
}
