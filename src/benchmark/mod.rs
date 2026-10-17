//! `strauss-rs benchmark`: Strauss-RS against Strauss PHP, for people building
//! from source.
//!
//! This is only built with the `benchmark` feature, which is off by default and
//! which the release workflow never turns on, so none of it is in a release.
//!
//! ```text
//! cargo run --release --features benchmark -- benchmark setup
//! cargo run --release --features benchmark -- benchmark run
//! ```
//!
//! Each project is prefixed by each tool with one worker and with many, from a
//! clean target directory. The first run of each is thrown away, the rest are
//! timed, and the output is checked.

mod check;
mod measure;
mod projects;
mod report;
mod setup;
mod tools;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use clap::{Args as ClapArgs, Subcommand};

use check::check;
use measure::{measure, median};
use projects::{Manifest, Project, config_problems, select};
use report::{Environment, Report, Row, Settings, format_ms};
use tools::{Launcher, Tool, is_wrapper_script, locate};

pub(crate) type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Sub,
}

#[derive(Debug, Subcommand)]
enum Sub {
    /// Download Strauss PHP and install each project's packages.
    ///
    /// Needs `curl` and Composer, and the network.
    Setup(SetupArgs),

    /// Prefix each project with each tool, one worker against one worker and
    /// many against many, and report the time and memory.
    ///
    /// Use a release build, and a `php` that is a PHP binary and not a script
    /// that starts PHP in a container.
    Run(RunArgs),
}

#[derive(Debug, ClapArgs)]
struct Common {
    /// The benchmark directory, with `benchmark.json` and the projects.
    #[arg(long, value_name = "DIR", default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/bench"))]
    dir: PathBuf,

    /// Only this project. Can be given more than once.
    #[arg(long = "project", value_name = "NAME")]
    projects: Vec<String>,
}

#[derive(Debug, ClapArgs)]
struct SetupArgs {
    #[command(flatten)]
    common: Common,
}

#[derive(Debug, ClapArgs)]
struct RunArgs {
    #[command(flatten)]
    common: Common,

    /// Only this tool. Can be given more than once.
    #[arg(long = "tool", value_enum)]
    tools: Vec<Tool>,

    /// How many timed runs of each.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..))]
    runs: u32,

    /// How many runs to throw away first.
    #[arg(long, default_value_t = 1)]
    warmup: u32,

    /// The workers in the runs with many. By default one per CPU.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(2..))]
    workers: Option<u32>,

    /// The PHP binary that runs Strauss PHP and checks the output.
    #[arg(long, value_name = "PATH", default_value = "php")]
    php: String,

    /// Write the results, and what they were measured on, to this JSON file.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,

    /// Don't parse the output with PHP.
    #[arg(long)]
    no_check: bool,

    /// Don't take the extra run that adds up the memory of every process.
    #[arg(long)]
    no_tree_memory: bool,

    /// Leave the last output in place, instead of deleting it.
    #[arg(long)]
    keep: bool,

    /// Run even though this is a debug build, which makes the times meaningless.
    #[arg(long)]
    allow_debug: bool,
}

pub fn execute(args: Args) -> Result {
    match args.command {
        Sub::Setup(args) => {
            let manifest = Manifest::load(&args.common.dir)?;
            let projects = select(&args.common.dir, &manifest, &args.common.projects)?;

            setup::setup(&args.common.dir, &manifest, &projects)
        }

        Sub::Run(args) => run(&args),
    }
}

fn run(args: &RunArgs) -> Result {
    if cfg!(debug_assertions) && !args.allow_debug {
        return Err("this is a debug build, so the times would mean nothing; run it with `cargo run --release --features benchmark -- benchmark run` (or pass --allow-debug)".into());
    }

    let root = &args.common.dir;
    let manifest = Manifest::load(root)?;
    let projects = select(root, &manifest, &args.common.projects)?;

    for project in &projects {
        if !project.is_installed() {
            return Err(format!(
                "the packages of `{}` are not installed; run `benchmark setup` first",
                project.name
            )
            .into());
        }

        let problems = config_problems(project);

        if !problems.is_empty() {
            return Err(format!(
                "the settings for `{}` ask the two tools for different work: {}",
                project.name,
                problems.join("; ")
            )
            .into());
        }
    }

    let tools = if args.tools.is_empty() {
        vec![Tool::StraussPhp, Tool::StraussRs]
    } else {
        args.tools.clone()
    };

    let php = locate(&args.php);
    let phar = manifest.phar(root);

    if tools.contains(&Tool::StraussPhp) {
        if php.is_none() {
            return Err(format!("could not find `{}`; pass --php", args.php).into());
        }

        if !phar.is_file() {
            return Err("Strauss PHP is not downloaded; run `benchmark setup` first".into());
        }
    }

    let launcher = Launcher {
        exe: env::current_exe()?,
        php: php.clone().unwrap_or_default(),
        phar,
    };

    let cpus = thread::available_parallelism().map_or(1, |cpus| cpus.get());
    let multi = args.workers.map_or(cpus, |workers| workers as usize);

    let mut worker_counts = vec![1];

    if multi > 1 {
        worker_counts.push(multi);
    }

    let environment = environment(&manifest, php.as_deref());

    let settings = Settings {
        runs: args.runs as usize,
        warmup: args.warmup as usize,
        multi_workers: multi,
    };

    if environment.php_is_wrapper && tools.contains(&Tool::StraussPhp) {
        eprintln!(
            "warning: `{}` is a script, so Strauss PHP is probably run in a container; the figures will not be real\n",
            args.php
        );
    }

    let parse_with = php.as_deref().filter(|_| !args.no_check);
    let mut rows = Vec::new();

    for project in &projects {
        for &workers in &worker_counts {
            for &tool in &tools {
                eprint!(
                    "{} / {} / {workers} worker{} ... ",
                    project.name,
                    tool.label(),
                    if workers == 1 { "" } else { "s" }
                );

                let row = bench(
                    &launcher, project, tool, workers, args, &settings, parse_with,
                );

                match (&row.failure, row.median_ms()) {
                    (Some(failure), _) => eprintln!("FAILED ({failure})"),
                    (None, Some(ms)) => eprintln!("{}", format_ms(ms)),
                    (None, None) => eprintln!("no runs"),
                }

                rows.push(row);
            }
        }
    }

    let report = Report {
        environment,
        settings,
        rows,
    };

    println!("{}", report::render(&report));

    eprintln!();

    for note in report::notes(&report) {
        eprintln!("* {note}");
    }

    if let Some(path) = &args.json {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, serde_json::to_string_pretty(&report)? + "\n")?;

        eprintln!("\nWrote {}", path.display());
    }

    Ok(())
}

/// One project with one tool and one number of workers.
fn bench(
    launcher: &Launcher,
    project: &Project,
    tool: Tool,
    workers: usize,
    args: &RunArgs,
    settings: &Settings,
    parse_with: Option<&Path>,
) -> Row {
    let mut row = Row {
        project: project.name.clone(),
        tool,
        workers,
        wall_ms: Vec::new(),
        peak_process_bytes: None,
        peak_tree_bytes: None,
        php_files: 0,
        parse_errors: None,
        failure: None,
    };

    let output = project.output();

    let attempt = |sample: bool| {
        clean(&output);

        let mut command = launcher.command(tool, &project.dir, workers);

        measure(&mut command, sample)
    };

    let describe = |measured: &measure::Measured| match measured.exit_code {
        Some(code) => format!("exit {code}"),
        None => "killed".to_string(),
    };

    for _ in 0..settings.warmup {
        match attempt(false) {
            Ok(measured) if measured.success => {}
            Ok(measured) => {
                row.failure = Some(describe(&measured));
                clean(&output);

                return row;
            }
            Err(error) => {
                row.failure = Some(format!("could not start: {error}"));

                return row;
            }
        }
    }

    for run in 0..settings.runs {
        let measured = match attempt(false) {
            Ok(measured) => measured,
            Err(error) => {
                row.failure = Some(format!("could not start: {error}"));

                return row;
            }
        };

        if !measured.success {
            row.failure = Some(describe(&measured));
            clean(&output);

            return row;
        }

        row.wall_ms.push(measured.wall.as_secs_f64() * 1000.0);
        row.peak_process_bytes = row.peak_process_bytes.max(measured.peak_process);

        // The output of the first run stands for them all.
        if run == 0 {
            let checked = check(&output, parse_with);

            row.php_files = checked.php_files;
            row.parse_errors = checked.parse_errors;
        }
    }

    if !args.no_tree_memory
        && let Ok(measured) = attempt(true)
    {
        // Sampled, so it can come out under the exact figure for the largest
        // process, which the whole tree can't be.
        row.peak_tree_bytes = measured
            .peak_tree
            .max(measured.peak_process)
            .max(row.peak_process_bytes);
    }

    if !args.keep {
        clean(&output);
    }

    row
}

fn clean(output: &Path) {
    let _ = fs::remove_dir_all(output);
}

fn environment(manifest: &Manifest, php: Option<&Path>) -> Environment {
    let php_start = php.map(|php| {
        let starts: Vec<_> = (0..5)
            .filter_map(|_| measure(Command::new(php).args(["-r", ""]), false).ok())
            .map(|measured| measured.wall)
            .collect();

        median(&starts).map(|wall| wall.as_secs_f64() * 1000.0)
    });

    let php_version = php.and_then(|php| {
        let output = Command::new(php)
            .args(["-r", "echo PHP_VERSION;"])
            .output()
            .ok()?;

        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    });

    Environment {
        os: os_name(),
        cpu: cpu_model(),
        logical_cpus: thread::available_parallelism().map_or(1, |cpus| cpus.get()),
        memory_bytes: total_memory(),
        strauss_rs_version: env!("CARGO_PKG_VERSION"),
        strauss_rs_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        strauss_php_version: manifest.strauss_php.version.clone(),
        php_version,
        php_is_wrapper: php.is_some_and(is_wrapper_script),
        php_start_ms: php_start.flatten(),
    }
}

fn os_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME="))
                .map(|name| name.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| env::consts::OS.to_string())
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("model name"))
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, name)| name.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn total_memory() -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}
