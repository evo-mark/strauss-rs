//! The command line interface.

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use php_rs_parser::parse;

use crate::analyser;
use crate::commands::{self, explain};
use crate::config::{Config, Parallel, UpdateCallSites};
use crate::edits::{self, EditPlan};
use crate::output;
use crate::packages::{self, PackageFile, PackageSet};
use crate::rename::{self, PlannedSymbol};
use crate::symbols::{self, SymbolKind, SymbolTable};

#[derive(Debug, Parser)]
#[command(
    name = "strauss-rs",
    version,
    about = "Prefix the namespaces of a Wordpress plugin or theme's dependencies",
    arg_required_else_help = true
)]
struct Cli {
    /// Show no progress and no summary, only warnings and errors.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the resolved configuration as JSON.
    ///
    /// Every option is shown with its default or inferred value filled in, in
    /// the form `strauss.json` takes, after applying any options given here.
    Config(ConfigArgs),

    /// List the packages to process as JSON: those in `packages`, and
    /// everything they require.
    ///
    /// Reads `vendor/composer/installed.json`, so run `composer install`
    /// first. Warnings go to stderr.
    Packages(ProjectArgs),

    /// List the files of those packages as JSON, and which would be copied.
    ///
    /// Every file in a package is listed, except those git would leave out of
    /// an archive: `.git`, `.gitignore`d files and `export-ignore` files.
    Files(ScanArgs),

    /// List the symbols each package declares as JSON: namespaces, classes,
    /// interfaces, traits, enums, functions and constants.
    ///
    /// Analyses every `.php` file of every package, so it can take a moment.
    /// PHP's own symbols, which polyfills redeclare, are listed separately.
    /// Files that can't be analysed are reported on stderr.
    Symbols(ScanArgs),

    /// Show which symbols would be renamed, to what, and why not the others, as
    /// JSON.
    ///
    /// Analyses every package like `symbols` does. Without `--package` this is
    /// every symbol, which for a large project is a lot; `summary` has the
    /// counts.
    Plan(PlanArgs),

    /// Explain what happens to one symbol, and why.
    ///
    /// Shows whether a class, function, constant or namespace is renamed, what
    /// to, and which setting decided it. Symbols that PHP or an
    /// `exclude_presets` preset provides are explained too, even when no package
    /// declares them. Exits with an error if nothing matches.
    Explain(ExplainArgs),

    /// Write a starting `strauss.json`.
    ///
    /// Works out the prefixes from `composer.json`. Dependencies go under
    /// `<your namespace>\Vendor` so they can't meet your own classes. Refuses to
    /// replace an existing `strauss.json` without `--force`.
    Init(InitArgs),

    /// Move PHP Strauss' `extra.strauss` from `composer.json` into
    /// `strauss.json`.
    ///
    /// Checks the options are ones strauss-rs knows before writing anything,
    /// then removes `extra.strauss` so there is one source of truth. Keeps
    /// existing class names by setting `classmap_prefix` where PHP Strauss
    /// would have inferred it differently.
    Migrate(MigrateArgs),

    /// Show the changes that would be made to the files, as a diff.
    ///
    /// Nothing is written. Runs the same analysis as `plan`, then works out
    /// every place that has to change so that the renamed symbols still work.
    /// The summary and any warnings go to stderr.
    Diff(DiffArgs),

    /// Prefix the packages: copy them to `target_directory` with their
    /// namespaces and global symbols renamed, and write the autoloader.
    ///
    /// Does what `diff` shows, to the files. `vendor` is left as it is unless
    /// `delete_vendor_files` or `delete_vendor_packages` is set. A summary and
    /// any warnings go to stderr.
    Prefix(PrefixArgs),

    /// Make `vendor/autoload.php` load the prefixed packages' autoloader too.
    ///
    /// Adds a `require_once` of the target directory's `autoload.php` before
    /// Composer's loader is returned. Safe to run again. Run `prefix` first so
    /// there is an autoloader to include.
    IncludeAutoloader(IncludeArgs),

    /// Development only: time and measure Strauss-RS against Strauss PHP.
    ///
    /// Built only with `--features benchmark`, so it is not in a release.
    #[cfg(feature = "benchmark")]
    Benchmark(crate::benchmark::Args),

    /// Analyse a PHP file and print the symbols it declares and refers to.
    Analyse {
        /// The PHP file to analyse.
        file: PathBuf,
    },
}

/// Finds the project. Shared by every command that reads `strauss.json`.
#[derive(Debug, Args)]
struct ProjectArgs {
    /// The plugin or theme root, containing `strauss.json` and `composer.json`.
    #[arg(short = 'C', long, value_name = "DIR", default_value = ".")]
    project_dir: PathBuf,
}

/// How many workers analyse files. The commands that analyse files take it.
#[derive(Debug, Args)]
struct ParallelArgs {
    /// How many workers analyse files, overriding `parallel`: `auto` (or
    /// `true`) for one per available CPU, `false` or `1` for a single worker,
    /// or a number to cap the workers.
    #[arg(
        long,
        value_name = "auto|false|N",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    parallel: Option<Parallel>,
}

/// A project, and how to analyse its files.
#[derive(Debug, Args)]
struct ScanArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    parallel: ParallelArgs,
}

/// A project, and whether to write the change.
#[derive(Debug, Args)]
struct IncludeArgs {
    #[command(flatten)]
    project: ProjectArgs,

    /// Work out what would change, and write nothing.
    #[arg(long)]
    dry_run: bool,
}

/// A project, how to analyse its files, and which symbols to show.
#[derive(Debug, Args)]
struct PlanArgs {
    #[command(flatten)]
    scan: ScanArgs,

    /// Only show the symbols this package declares. Can be given more than
    /// once.
    #[arg(long = "package", value_name = "NAME")]
    packages: Vec<String>,
}

/// `--updateCallSites`, for the commands that plan changes to files.
#[derive(Debug, Args)]
struct CallSitesArgs {
    /// Which project files to update where they use prefixed symbols,
    /// overriding `update_call_sites`: `true` for the files in the project's
    /// `autoload`, `false` for none, or a comma-separated list of files and
    /// directories.
    #[arg(
        long = "updateCallSites",
        alias = "update-call-sites",
        value_name = "true|false|PATHS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    update_call_sites: Option<UpdateCallSites>,
}

/// A project, how to analyse its files, and which files to show.
#[derive(Debug, Args)]
struct DiffArgs {
    #[command(flatten)]
    scan: ScanArgs,

    #[command(flatten)]
    call_sites: CallSitesArgs,

    /// Only show the files of this package. Can be given more than once.
    #[arg(long = "package", value_name = "NAME")]
    packages: Vec<String>,

    /// Show the edits as JSON, with their positions, instead of a diff.
    #[arg(long)]
    json: bool,
}

/// A project, how to analyse its files, and whether to write the result.
#[derive(Debug, Args)]
struct PrefixArgs {
    #[command(flatten)]
    scan: ScanArgs,

    #[command(flatten)]
    call_sites: CallSitesArgs,

    /// Work out what would be written, and write nothing.
    #[arg(long)]
    dry_run: bool,

    /// Print what was written as JSON, instead of leaving it to the summary
    /// on stderr.
    #[arg(long)]
    json: bool,
}

/// The kinds of symbol `explain --kind` can narrow to.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum KindArg {
    Namespace,
    Class,
    Interface,
    Trait,
    Enum,
    Function,
    Constant,
}

impl From<KindArg> for SymbolKind {
    fn from(kind: KindArg) -> SymbolKind {
        match kind {
            KindArg::Namespace => SymbolKind::Namespace,
            KindArg::Class => SymbolKind::Class,
            KindArg::Interface => SymbolKind::Interface,
            KindArg::Trait => SymbolKind::Trait,
            KindArg::Enum => SymbolKind::Enum,
            KindArg::Function => SymbolKind::Function,
            KindArg::Constant => SymbolKind::Constant,
        }
    }
}

/// A project, and the symbol to explain.
#[derive(Debug, Args)]
struct ExplainArgs {
    #[command(flatten)]
    scan: ScanArgs,

    /// The symbol's full name, such as `GuzzleHttp\Client`, `my_function` or
    /// `ABSPATH`. A leading backslash is ignored.
    symbol: String,

    /// Only consider this kind of symbol, for a name that is more than one.
    #[arg(long, value_enum)]
    kind: Option<KindArg>,

    /// Print the explanation as JSON.
    #[arg(long)]
    json: bool,
}

/// A project, and what to write.
#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    project: ProjectArgs,

    /// Replace `strauss.json` if there is one.
    #[arg(long)]
    force: bool,

    /// Show what would be written, and write nothing.
    #[arg(long)]
    dry_run: bool,

    /// The `namespace_prefix` to write, instead of one worked out from
    /// `composer.json`.
    #[arg(long, value_name = "PREFIX")]
    namespace_prefix: Option<String>,

    /// The `classmap_prefix` to write, instead of one worked out from
    /// `composer.json`.
    #[arg(long, value_name = "PREFIX")]
    classmap_prefix: Option<String>,

    /// The `target_directory` to write, if not `vendor-prefixed`.
    #[arg(long, value_name = "DIR")]
    target_directory: Option<String>,

    /// Add scripts to `composer.json` that run `prefix` after `composer install`
    /// and `composer update`, without asking.
    #[arg(long, conflicts_with = "no_composer_scripts")]
    composer_scripts: bool,

    /// Don't add the Composer scripts, and don't ask.
    #[arg(long)]
    no_composer_scripts: bool,
}

/// A project, and whether to write the move.
#[derive(Debug, Args)]
struct MigrateArgs {
    #[command(flatten)]
    project: ProjectArgs,

    /// Replace `strauss.json` if there is one.
    #[arg(long)]
    force: bool,

    /// Show what would be written, and write nothing.
    #[arg(long)]
    dry_run: bool,
}

/// A project, and options that override its configuration.
#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(flatten)]
    project: ProjectArgs,

    #[command(flatten)]
    overrides: Overrides,
}

/// Options that take precedence over `strauss.json`.
#[derive(Debug, Args)]
struct Overrides {
    /// Which project files to update where they use prefixed symbols,
    /// overriding `update_call_sites`: `true` for the files in the project's
    /// `autoload`, `false` for none, or a comma-separated list of files and
    /// directories.
    #[arg(
        long = "updateCallSites",
        alias = "update-call-sites",
        value_name = "true|false|PATHS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    update_call_sites: Option<UpdateCallSites>,

    #[command(flatten)]
    parallel: ParallelArgs,
}

/// `--quiet`: the summaries are left out.
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Prints a summary line to stderr, unless `--quiet`.
macro_rules! summary {
    ($($argument:tt)*) => {
        if !QUIET.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!($($argument)*);
        }
    };
}

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn run() -> ExitCode {
    let cli = Cli::parse();

    // Progress is drawn only for a person watching: not when stderr is piped or
    // logged.
    if !cli.quiet && std::io::stderr().is_terminal() {
        crate::progress::enable();
    }

    QUIET.store(cli.quiet, std::sync::atomic::Ordering::Relaxed);

    match execute(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(command: Command) -> Result {
    match command {
        Command::Config(args) => {
            println!("{}", render_config(&args)?);

            Ok(())
        }

        Command::Packages(args) => {
            println!("{}", render_packages(&args)?);

            Ok(())
        }

        Command::Files(args) => {
            println!("{}", render_files(&args)?);

            Ok(())
        }

        Command::Symbols(args) => {
            println!("{}", render_symbols(&args)?);

            Ok(())
        }

        Command::Plan(args) => {
            println!("{}", render_plan(&args)?);

            Ok(())
        }

        Command::Diff(args) => {
            let output = render_diff(&args)?;

            if !output.is_empty() {
                println!("{output}");
            }

            Ok(())
        }

        Command::Explain(args) => explain_symbol(&args),

        Command::Init(args) => init(&args),

        Command::Migrate(args) => migrate(&args),

        Command::Prefix(args) => prefix(&args),

        Command::IncludeAutoloader(args) => include_autoloader(&args),

        #[cfg(feature = "benchmark")]
        Command::Benchmark(args) => crate::benchmark::execute(args),

        Command::Analyse { file } => analyse_file(&file),
    }
}

/// The configuration as `strauss.json` would spell it out.
fn render_config(args: &ConfigArgs) -> Result<String> {
    let mut config = load_config(&args.project)?;

    if let Some(update_call_sites) = &args.overrides.update_call_sites {
        config.update_call_sites = update_call_sites.clone();
    }

    if let Some(parallel) = args.overrides.parallel.parallel {
        config.parallel = parallel;
    }

    Ok(serde_json::to_string_pretty(&config)?)
}

fn render_packages(args: &ProjectArgs) -> Result<String> {
    let config = load_config(args)?;
    let set = packages::enumerate_packages(&config)?;

    report_problems(&set);

    Ok(serde_json::to_string_pretty(&set)?)
}

fn render_files(args: &ScanArgs) -> Result<String> {
    let mut config = load_config(&args.project)?;

    if let Some(parallel) = args.parallel.parallel {
        config.parallel = parallel;
    }

    let set = packages::enumerate_packages(&config)?;
    let files = packages::enumerate_files(&config, &set)?;

    report_problems(&set);

    Ok(serde_json::to_string_pretty(
        &serde_json::json!({ "files": files }),
    )?)
}

fn render_symbols(args: &ScanArgs) -> Result<String> {
    let mut config = load_config(&args.project)?;

    if let Some(parallel) = args.parallel.parallel {
        config.parallel = parallel;
    }

    let set = packages::enumerate_packages(&config)?;
    let files = packages::enumerate_files(&config, &set)?;
    let table = symbols::scan(&config, &files);

    report_problems(&set);
    report_scan_warnings(&table);

    Ok(serde_json::to_string_pretty(&table.report())?)
}

/// What `strauss-rs plan` prints.
#[derive(serde::Serialize)]
struct PlanReport<'a> {
    packages_to_prefix: &'a [String],
    summary: rename::PlanSummary,
    files_excluded_from_prefix: &'a [rename::ExcludedFile],
    symbols: Vec<&'a PlannedSymbol>,
}

fn render_plan(args: &PlanArgs) -> Result<String> {
    let mut config = load_config(&args.scan.project)?;

    if let Some(parallel) = args.scan.parallel.parallel {
        config.parallel = parallel;
    }

    let set = packages::enumerate_packages(&config)?;
    let mut files = packages::enumerate_files(&config, &set)?;
    let table = symbols::scan(&config, &files);

    rename::exclude_namespaces_from_copy(&config, &mut files, &table);

    let plan = rename::plan(&config, &set, &files, &table);

    report_problems(&set);
    report_scan_warnings(&table);

    let selected: Vec<&PlannedSymbol> = plan
        .symbols
        .iter()
        .filter(|symbol| {
            args.packages.is_empty()
                || symbol.packages.iter().any(|package| {
                    args.packages
                        .iter()
                        .any(|wanted| wanted.eq_ignore_ascii_case(package))
                })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&PlanReport {
        packages_to_prefix: &plan.packages_to_prefix,
        summary: plan.summary(),
        files_excluded_from_prefix: &plan.files_excluded_from_prefix,
        symbols: selected,
    })?)
}

fn explain_symbol(args: &ExplainArgs) -> Result {
    let mut config = load_config(&args.scan.project)?;

    if let Some(parallel) = args.scan.parallel.parallel {
        config.parallel = parallel;
    }

    let set = packages::enumerate_packages(&config)?;
    let mut files = packages::enumerate_files(&config, &set)?;
    let table = symbols::scan(&config, &files);

    rename::exclude_namespaces_from_copy(&config, &mut files, &table);

    let plan = rename::plan(&config, &set, &files, &table);

    report_problems(&set);
    report_scan_warnings(&table);

    let explanation = explain::explain(
        &config,
        &table,
        &plan,
        &args.symbol,
        args.kind.map(SymbolKind::from),
    )
    .map_err(|not_found| not_found.message)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&explanation)?);
    } else {
        println!("{}", explain::render(&explanation));
    }

    Ok(())
}

/// The project directory, which has to exist for `init` and `migrate` to
/// write into it.
fn project_directory(args: &ProjectArgs) -> Result<PathBuf> {
    let directory = fs::canonicalize(&args.project_dir).map_err(|error| {
        format!(
            "could not open project directory {}: {error}",
            args.project_dir.display()
        )
    })?;

    if !directory.is_dir() {
        return Err(format!("{} is not a directory", directory.display()).into());
    }

    Ok(directory)
}

/// Asks a yes or no question on stderr, when a person is there to answer.
/// `false` when stdin isn't a terminal.
fn confirm(question: &str) -> bool {
    use std::io::Write;

    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return false;
    }

    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();

    let mut answer = String::new();

    std::io::stdin().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn init(args: &InitArgs) -> Result {
    let project_dir = project_directory(&args.project)?;

    let composer_scripts = if args.composer_scripts {
        true
    } else if args.no_composer_scripts || args.dry_run {
        false
    } else {
        confirm(&format!(
            "Add scripts to composer.json that run `{}` after install and update?",
            commands::composer_file::PREFIX_COMMAND
        ))
    };

    let outcome = commands::init::run(&commands::init::Options {
        project_dir,
        force: args.force,
        dry_run: args.dry_run,
        namespace_prefix: args.namespace_prefix.clone(),
        classmap_prefix: args.classmap_prefix.clone(),
        target_directory: args.target_directory.clone(),
        composer_scripts,
    })?;

    if args.dry_run {
        print!("{}", outcome.strauss_json);
        summary!("Would write strauss.json");
    } else {
        summary!("Wrote strauss.json");
    }

    for change in &outcome.composer_changes {
        summary!(
            "{} composer.json: {change}",
            if args.dry_run {
                "Would change"
            } else {
                "Changed"
            }
        );
    }

    // Notes are advice, so they are left out with the other summaries.
    for note in &outcome.notes {
        summary!("note: {note}");
    }

    Ok(())
}

fn migrate(args: &MigrateArgs) -> Result {
    let project_dir = project_directory(&args.project)?;

    let outcome = commands::migrate::run(&commands::migrate::Options {
        project_dir,
        force: args.force,
        dry_run: args.dry_run,
    })?;

    if args.dry_run {
        print!("{}", outcome.strauss_json);
        summary!("Would write strauss.json and remove extra.strauss from composer.json");
    } else {
        summary!("Wrote strauss.json and removed extra.strauss from composer.json");
    }

    for note in &outcome.notes {
        // These change what happens next, so they are warnings.
        eprintln!("warning: {note}");
    }

    Ok(())
}

/// Everything worked out before anything is written.
struct Analysed {
    config: Config,
    set: PackageSet,
    files: Vec<PackageFile>,
    rename: rename::RenamePlan,
    edits: EditPlan,
}

/// Runs the analysis that `diff` and `prefix` share, reporting problems on
/// stderr.
fn analyse_project(args: &ScanArgs, call_sites: &CallSitesArgs) -> Result<Analysed> {
    let mut config = load_config(&args.project)?;

    if let Some(update_call_sites) = &call_sites.update_call_sites {
        config.update_call_sites = update_call_sites.clone();
    }

    if let Some(parallel) = args.parallel.parallel {
        config.parallel = parallel;
    }

    let set = {
        let _step = crate::progress::spinner("Packages");

        packages::enumerate_packages(&config)?
    };

    let mut files = {
        let _step = crate::progress::spinner("Listing files");

        packages::enumerate_files(&config, &set)?
    };

    let table = symbols::scan(&config, &files);

    let rename = {
        let _step = crate::progress::spinner("Renaming");

        rename::exclude_namespaces_from_copy(&config, &mut files, &table);

        rename::plan(&config, &set, &files, &table)
    };

    let edits = edits::plan(&config, &files, &table, &rename);

    report_problems(&set);
    report_scan_warnings(&table);

    for warning in &edits.warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(Analysed {
        config,
        set,
        files,
        rename,
        edits,
    })
}

fn render_diff(args: &DiffArgs) -> Result<String> {
    let Analysed {
        edits: mut edit_plan,
        ..
    } = analyse_project(&args.scan, &args.call_sites)?;

    if !args.packages.is_empty() {
        edit_plan.files.retain(|file| {
            args.packages
                .iter()
                .any(|wanted| wanted.eq_ignore_ascii_case(&file.package))
        });

        edit_plan.call_sites.clear();
    }

    summary!(
        "{} files to change with {} edits; {} unchanged; {} skipped",
        edit_plan.files.len() + edit_plan.call_sites.len(),
        edit_plan.edit_count(),
        edit_plan.unchanged_files,
        edit_plan.skipped.len()
    );

    if args.json {
        return Ok(serde_json::to_string_pretty(&edit_plan)?);
    }

    let mut output = String::new();

    for file in edit_plan.files.iter().chain(&edit_plan.call_sites) {
        let original = fs::read_to_string(&file.source_path)
            .map_err(|error| format!("could not read {}: {error}", file.source_path.display()))?;

        let changed = edits::apply_edits(&original, &file.edits)
            .map_err(|error| format!("{}: {error}", file.file))?;

        let diff = similar::TextDiff::from_lines(&original, &changed);

        output.push_str(
            &diff
                .unified_diff()
                .context_radius(2)
                .header(&format!("a/{}", file.file), &format!("b/{}", file.file))
                .to_string(),
        );
    }

    Ok(output.trim_end().to_string())
}

fn prefix(args: &PrefixArgs) -> Result {
    let analysed = analyse_project(&args.scan, &args.call_sites)?;

    let report = output::write(
        &output::Input {
            config: &analysed.config,
            packages: &analysed.set,
            files: &analysed.files,
            rename: &analysed.rename,
            edits: &analysed.edits,
        },
        &output::Options::new(&analysed.config, args.dry_run, output::today()),
    )?;

    // "<file> was copied as it is: <reason>" is grouped by the reason.
    let warnings: Vec<(String, String)> = report
        .warnings
        .iter()
        .map(
            |warning| match warning.split_once(" was copied as it is: ") {
                Some((file, reason)) => (file.to_string(), format!("copied as it is: {reason}")),
                None => (String::new(), warning.clone()),
            },
        )
        .collect();

    print_grouped_warnings(
        warnings
            .iter()
            .map(|(file, message)| (file.as_str(), message.as_str())),
    );

    let verb = if args.dry_run { "Would write" } else { "Wrote" };

    if analysed.config.include_autoloader {
        if report.autoloader.is_empty() {
            eprintln!(
                "warning: include_autoloader: no autoloader was written, so there is nothing to include (see classmap_output)"
            );
        } else {
            match output::include_autoloader(&analysed.config, args.dry_run) {
                Ok(output::Included::Added) => summary!(
                    "{} {}/autoload.php to load the prefixed packages",
                    if args.dry_run {
                        "Would update"
                    } else {
                        "Updated"
                    },
                    analysed.config.vendor_directory.trim_matches('/')
                ),
                Ok(output::Included::AlreadyIncluded) => {}
                Err(error) => eprintln!("warning: include_autoloader: {error}"),
            }
        }
    }

    if !report.packages.is_empty() {
        // Colour is for a person watching, and `NO_COLOR` turns it off.
        let colour = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();

        summary!("{}", package_table(&report.packages, colour));
    }

    summary!(
        "{verb} {} files to {} ({} modified)",
        report.written,
        analysed.config.target_directory,
        report.modified
    );

    if !report.autoloader.is_empty() {
        summary!(
            "{verb} {} with {} classes",
            report.autoloader.join(" and "),
            report.classmap_entries
        );
    }

    if report.call_sites_updated > 0 {
        summary!(
            "{} {} of your own files",
            if args.dry_run {
                "Would update"
            } else {
                "Updated"
            },
            report.call_sites_updated
        );
    }

    if report.deleted_files > 0 {
        summary!(
            "{} {} files from {}{}",
            if args.dry_run {
                "Would delete"
            } else {
                "Deleted"
            },
            report.deleted_files,
            analysed.config.vendor_directory,
            if report.deleted_packages.is_empty() {
                String::new()
            } else {
                format!(
                    ", including {} whole packages",
                    report.deleted_packages.len()
                )
            }
        );
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }

    Ok(())
}

/// The packages as a table: what was written and changed for each, with a
/// total row.
fn package_table(packages: &[output::PackageReport], colour: bool) -> String {
    let headers = ["Package", "Files", "Modified", "Edits"];

    let mut rows: Vec<[String; 4]> = packages
        .iter()
        .map(|package| {
            [
                package.name.clone(),
                package.written.to_string(),
                package.modified.to_string(),
                package.edits.to_string(),
            ]
        })
        .collect();

    rows.push([
        "Total".to_string(),
        packages
            .iter()
            .map(|p| p.written)
            .sum::<usize>()
            .to_string(),
        packages
            .iter()
            .map(|p| p.modified)
            .sum::<usize>()
            .to_string(),
        packages.iter().map(|p| p.edits).sum::<usize>().to_string(),
    ]);

    let widths: [usize; 4] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .chain([headers[column].len()])
            .max()
            .unwrap_or(0)
    });

    // Cells are padded before they are coloured, so the codes don't count
    // towards the width.
    let paint = |text: String, code: &str| {
        if colour {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text
        }
    };

    let line = |cells: [&str; 4], styles: [&str; 4]| {
        let mut text = paint(
            format!("{:<width$}", cells[0], width = widths[0]),
            styles[0],
        );

        for column in 1..4 {
            text.push_str("  ");
            text.push_str(&paint(
                format!("{:>width$}", cells[column], width = widths[column]),
                styles[column],
            ));
        }

        text
    };

    let rule = paint("\u{2500}".repeat(widths.iter().sum::<usize>() + 2 * 3), "2");
    let (total, body) = rows.split_last().expect("the total row was added");

    let mut lines = vec![line(headers, ["1"; 4]), rule.clone()];

    lines.extend(body.iter().map(|row| {
        // Counts of nothing are dimmed, so what changed stands out.
        let count = |column: usize, code: &'static str| {
            if row[column] == "0" { "2" } else { code }
        };

        line(
            row.each_ref().map(String::as_str),
            ["36", "0", count(2, "32"), count(3, "32")],
        )
    }));
    lines.push(rule);
    lines.push(line(total.each_ref().map(String::as_str), ["1"; 4]));

    lines.join("\n")
}

fn include_autoloader(args: &IncludeArgs) -> Result {
    let config = load_config(&args.project)?;

    let vendor_autoload = format!("{}/autoload.php", config.vendor_directory.trim_matches('/'));
    let target_autoload = format!("{}/autoload.php", config.target_directory.trim_matches('/'));

    match output::include_autoloader(&config, args.dry_run)? {
        output::Included::Added => summary!(
            "{} {vendor_autoload} to load {target_autoload}",
            if args.dry_run {
                "Would update"
            } else {
                "Updated"
            }
        ),
        output::Included::AlreadyIncluded => {
            summary!("{vendor_autoload} already loads {target_autoload}")
        }
    }

    if !config.project_dir.join(&target_autoload).is_file() {
        eprintln!("warning: {target_autoload} does not exist yet; run `prefix` to write it");
    }

    Ok(())
}

fn report_scan_warnings(table: &SymbolTable) {
    print_grouped_warnings(
        table
            .warnings
            .iter()
            .map(|warning| (warning.file.as_str(), warning.message.as_str())),
    );
}

/// Prints `(file, message)` warnings, one line for each message that many
/// files share: a package of charset tables isn't a screenful.
fn print_grouped_warnings<'a>(warnings: impl Iterator<Item = (&'a str, &'a str)>) {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();

    for (file, message) in warnings {
        match groups.iter_mut().find(|(existing, _)| *existing == message) {
            Some((_, files)) => files.push(file),
            None => groups.push((message, vec![file])),
        }
    }

    for (message, files) in groups {
        if files.len() <= SHOWN_INDIVIDUALLY {
            for file in files {
                if file.is_empty() {
                    eprintln!("warning: {message}");
                } else {
                    eprintln!("warning: {file}: {message}");
                }
            }
        } else {
            eprintln!(
                "warning: {} files: {message} (such as {})",
                files.len(),
                files[..SHOWN_INDIVIDUALLY].join(", ")
            );
        }
    }
}

/// How many files with the same warning are listed before they are counted.
const SHOWN_INDIVIDUALLY: usize = 3;

/// Packages that were required but left out, or found with a problem, go to
/// stderr so the JSON on stdout stays clean.
fn report_problems(set: &PackageSet) {
    for skipped in &set.skipped {
        eprintln!("warning: {} was skipped: {}", skipped.name, skipped.reason);
    }

    for warning in &set.warnings {
        eprintln!("warning: {}: {}", warning.package, warning.message);
    }
}

/// Reads the project's configuration.
fn load_config(args: &ProjectArgs) -> Result<Config> {
    let project_dir = fs::canonicalize(&args.project_dir).map_err(|error| {
        format!(
            "could not open project directory {}: {error}",
            args.project_dir.display()
        )
    })?;

    if !project_dir.is_dir() {
        return Err(format!("{} is not a directory", project_dir.display()).into());
    }

    Ok(Config::load(&project_dir)?)
}

fn analyse_file(file: &Path) -> Result {
    let source = fs::read_to_string(file)
        .map_err(|error| format!("could not read {}: {error}", file.display()))?;

    let result = parse(&source);

    for error in &result.errors {
        eprintln!("Parse error: {error}");
    }

    let analysis = analyser::analyse(&source, &result.program);

    println!("{analysis:#?}");

    Ok(())
}

#[cfg(test)]
mod tests;
