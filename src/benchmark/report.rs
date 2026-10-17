//! The results: what was measured and on what, as a table and as JSON.

use std::time::Duration;

use serde::Serialize;

use crate::benchmark::measure::median;
use crate::benchmark::tools::Tool;

#[derive(Debug, Serialize)]
pub struct Environment {
    pub os: String,
    pub cpu: String,
    pub logical_cpus: usize,
    pub memory_bytes: Option<u64>,
    pub strauss_rs_version: &'static str,
    pub strauss_rs_profile: &'static str,
    pub strauss_php_version: String,
    pub php_version: Option<String>,

    /// The `php` is a script that starts PHP elsewhere, such as in a container.
    pub php_is_wrapper: bool,

    /// How long `php -r ''` takes: what starting PHP costs before it does any
    /// work, and the least a Strauss PHP row can show.
    pub php_start_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Settings {
    pub runs: usize,
    pub warmup: usize,

    /// The workers in the rows with many.
    pub multi_workers: usize,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub project: String,
    pub tool: Tool,
    pub workers: usize,

    /// Wall time of each timed run, in milliseconds.
    pub wall_ms: Vec<f64>,

    /// Peak memory of the largest process in any timed run, in bytes.
    pub peak_process_bytes: Option<u64>,

    /// Peak memory of the whole process tree in one extra run, in bytes.
    pub peak_tree_bytes: Option<u64>,

    pub php_files: usize,

    /// Files PHP could not parse. `None` if not checked.
    pub parse_errors: Option<Vec<String>>,

    /// Why the tool failed, if it did.
    pub failure: Option<String>,
}

impl Row {
    fn durations(&self) -> Vec<Duration> {
        self.wall_ms
            .iter()
            .map(|ms| Duration::from_secs_f64(ms / 1000.0))
            .collect()
    }

    pub fn median_ms(&self) -> Option<f64> {
        median(&self.durations()).map(|d| d.as_secs_f64() * 1000.0)
    }

    fn min_ms(&self) -> Option<f64> {
        self.wall_ms.iter().copied().reduce(f64::min)
    }

    fn max_ms(&self) -> Option<f64> {
        self.wall_ms.iter().copied().reduce(f64::max)
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub environment: Environment,
    pub settings: Settings,
    pub rows: Vec<Row>,
}

pub fn format_ms(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// How many times faster `row` is than Strauss PHP with the same project and
/// workers, as text.
fn speedup(rows: &[Row], row: &Row) -> String {
    if row.tool == Tool::StraussPhp {
        return "baseline".to_string();
    }

    let baseline = rows.iter().find(|other| {
        other.tool == Tool::StraussPhp
            && other.project == row.project
            && other.workers == row.workers
            && other.failure.is_none()
    });

    match (
        baseline.and_then(Row::median_ms),
        row.median_ms(),
        &row.failure,
    ) {
        (Some(php), Some(own), None) if own > 0.0 => format!("{:.1}x", php / own),
        _ => "-".to_string(),
    }
}

fn check_text(row: &Row) -> String {
    if let Some(failure) = &row.failure {
        return format!("FAILED ({failure})");
    }

    match &row.parse_errors {
        None => "not parsed".to_string(),
        Some(errors) if errors.is_empty() => "ok".to_string(),
        Some(errors) => format!("{} parse errors", errors.len()),
    }
}

/// A Markdown table of the rows.
pub fn render(report: &Report) -> String {
    let headers = [
        "Project",
        "Tool",
        "Workers",
        "Wall (median)",
        "Range",
        "Peak RSS (largest process)",
        "Peak total (all processes)",
        "PHP files",
        "Output",
        "vs Strauss PHP",
    ];

    let body: Vec<Vec<String>> = report
        .rows
        .iter()
        .map(|row| {
            let ran = row.failure.is_none();

            vec![
                row.project.clone(),
                row.tool.label().to_string(),
                row.workers.to_string(),
                row.median_ms()
                    .filter(|_| ran)
                    .map_or("-".to_string(), format_ms),
                match (row.min_ms(), row.max_ms()) {
                    (Some(min), Some(max)) if ran => {
                        format!("{} to {}", format_ms(min), format_ms(max))
                    }
                    _ => "-".to_string(),
                },
                row.peak_process_bytes.map_or("-".to_string(), format_bytes),
                row.peak_tree_bytes.map_or("-".to_string(), format_bytes),
                row.php_files.to_string(),
                check_text(row),
                speedup(&report.rows, row),
            ]
        })
        .collect();

    table(
        &headers,
        &body,
        &[
            false, false, true, true, true, true, true, true, false, true,
        ],
    )
}

fn table(headers: &[&str], rows: &[Vec<String>], right: &[bool]) -> String {
    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain([headers[column].chars().count(), 3])
                .max()
                .unwrap_or(3)
        })
        .collect();

    let line = |cells: Vec<&str>| -> String {
        let cells: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = widths[column];

                if right[column] {
                    format!("{cell:>width$}")
                } else {
                    format!("{cell:<width$}")
                }
            })
            .collect();

        format!("| {} |", cells.join(" | "))
    };

    let separator: Vec<String> = widths
        .iter()
        .enumerate()
        .map(|(column, width)| {
            if right[column] {
                format!("{}:", "-".repeat(width - 1))
            } else {
                "-".repeat(*width)
            }
        })
        .collect();

    let mut lines = vec![line(headers.to_vec())];

    lines.push(format!("| {} |", separator.join(" | ")));
    lines.extend(
        rows.iter()
            .map(|row| line(row.iter().map(String::as_str).collect())),
    );

    lines.join("\n")
}

/// What the table can't say by itself.
pub fn notes(report: &Report) -> Vec<String> {
    let environment = &report.environment;
    let mut notes = Vec::new();

    if environment.strauss_rs_profile != "release" {
        notes.push(
            "Strauss-RS is a debug build, so its times mean little. Build with --release."
                .to_string(),
        );
    }

    if environment.php_is_wrapper {
        notes.push(
            "`php` is a script that starts PHP elsewhere, in a container perhaps. Strauss PHP's times include starting it, and its memory is the script's, not PHP's. Use --php with a PHP binary for real figures.".to_string(),
        );
    }

    notes.push(format!(
        "Workers: 1 against 1, and {} against {}. Both tools are asked for the number outright. Strauss PHP may start fewer if it holds one CPU back.",
        report.settings.multi_workers, report.settings.multi_workers
    ));

    notes.push(
        "Strauss PHP writes Composer's autoloader files and Strauss-RS writes its own lighter one, so the output is not file for file the same.".to_string(),
    );

    notes.push(
        "Peak RSS is exact. Peak total is sampled while the command runs and adds up every process it starts, using proportional memory where Linux gives it so forked workers don't count shared pages twice. It can miss a short peak, so it is never shown below Peak RSS. For Strauss-RS, which is one process, the two are the same.".to_string(),
    );

    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(project: &str, tool: Tool, workers: usize, wall_ms: &[f64]) -> Row {
        Row {
            project: project.to_string(),
            tool,
            workers,
            wall_ms: wall_ms.to_vec(),
            peak_process_bytes: Some(10 * 1024 * 1024),
            peak_tree_bytes: None,
            php_files: 5,
            parse_errors: Some(Vec::new()),
            failure: None,
        }
    }

    fn report(rows: Vec<Row>) -> Report {
        Report {
            environment: Environment {
                os: "linux".to_string(),
                cpu: "cpu".to_string(),
                logical_cpus: 4,
                memory_bytes: None,
                strauss_rs_version: "0",
                strauss_rs_profile: "release",
                strauss_php_version: "0.31.1".to_string(),
                php_version: None,
                php_is_wrapper: false,
                php_start_ms: None,
            },
            settings: Settings {
                runs: 3,
                warmup: 1,
                multi_workers: 4,
            },
            rows,
        }
    }

    #[test]
    fn times_are_shown_in_milliseconds_and_seconds() {
        assert_eq!(format_ms(12.34), "12.3 ms");
        assert_eq!(format_ms(1500.0), "1.50 s");
    }

    #[test]
    fn memory_is_shown_in_mebibytes() {
        assert_eq!(format_bytes(3 * 1024 * 1024 + 512 * 1024), "3.5 MiB");
    }

    #[test]
    fn the_speedup_compares_the_same_project_and_workers() {
        let rows = vec![
            row("small", Tool::StraussPhp, 1, &[1000.0, 1000.0, 1000.0]),
            row("small", Tool::StraussPhp, 4, &[400.0, 400.0, 400.0]),
            row("small", Tool::StraussRs, 1, &[50.0, 50.0, 50.0]),
            row("small", Tool::StraussRs, 4, &[10.0, 10.0, 10.0]),
        ];

        assert_eq!(speedup(&rows, &rows[0]), "baseline");
        assert_eq!(speedup(&rows, &rows[2]), "20.0x");
        assert_eq!(speedup(&rows, &rows[3]), "40.0x");
    }

    #[test]
    fn there_is_no_speedup_without_a_baseline_or_after_a_failure() {
        let alone = vec![row("small", Tool::StraussRs, 1, &[50.0])];

        assert_eq!(speedup(&alone, &alone[0]), "-");

        let mut failed = row("small", Tool::StraussRs, 1, &[50.0]);
        failed.failure = Some("exit 1".to_string());

        let rows = vec![row("small", Tool::StraussPhp, 1, &[1000.0]), failed];

        assert_eq!(speedup(&rows, &rows[1]), "-");
    }

    #[test]
    fn the_table_has_a_line_for_each_row_and_the_columns_line_up() {
        let text = render(&report(vec![
            row("small", Tool::StraussPhp, 1, &[1000.0, 1200.0, 1100.0]),
            row("small", Tool::StraussRs, 1, &[50.0, 60.0, 55.0]),
        ]));

        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines.len(), 4, "{text}");
        assert!(lines[2].contains("Strauss PHP"));
        assert!(lines[2].contains("1.10 s"));
        assert!(lines[3].contains("55.0 ms"));
        assert!(lines[3].contains("20.0x"));

        let widths: Vec<usize> = lines.iter().map(|line| line.chars().count()).collect();

        assert!(widths.iter().all(|width| *width == widths[0]), "{text}");
    }

    #[test]
    fn a_failed_run_is_marked_in_the_table() {
        let mut failed = row("large", Tool::StraussPhp, 1, &[]);
        failed.failure = Some("exit 255".to_string());

        let text = render(&report(vec![failed]));

        assert!(text.contains("FAILED (exit 255)"), "{text}");
    }

    #[test]
    fn the_notes_say_when_the_numbers_cannot_be_trusted() {
        let mut report = report(Vec::new());

        report.environment.strauss_rs_profile = "debug";
        report.environment.php_is_wrapper = true;

        let notes = notes(&report).join("\n");

        assert!(notes.contains("debug build"));
        assert!(notes.contains("starts PHP elsewhere"));
    }

    #[test]
    fn the_report_is_json() {
        let json =
            serde_json::to_value(report(vec![row("small", Tool::StraussRs, 1, &[1.0])])).unwrap();

        assert_eq!(json["rows"][0]["tool"], "strauss-rs");
        assert_eq!(json["settings"]["multi_workers"], 4);
    }
}
