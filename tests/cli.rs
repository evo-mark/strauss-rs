//! Runs the compiled binary, checking what a user (or a build script) sees: output, error
//! messages and exit codes.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn strauss(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strauss-rs"))
        .args(args)
        .output()
        .expect("failed to run strauss-rs")
}

fn path(relative: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), relative].iter().collect();

    path.to_string_lossy().into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn prints_the_resolved_configuration_as_json() {
    let output = strauss(&["config", "-C", &path("test/config-fixture")]);

    assert!(output.status.success(), "{}", stderr(&output));

    let config: Value = serde_json::from_str(&stdout(&output)).unwrap();

    assert_eq!(config["target_directory"], "lib/prefixed");
    assert_eq!(config["namespace_prefix"], "Acme\\Plugin\\Vendor");
    assert_eq!(config["classmap_prefix"], "Acme_Plugin_");
    assert_eq!(config["parallel"], "auto");
    assert_eq!(config["update_call_sites"], false);
}

#[test]
fn command_line_options_override_the_configuration() {
    let output = strauss(&[
        "config",
        "-C",
        &path("test/config-fixture"),
        "--updateCallSites=includes,templates",
        "--parallel=4",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    let config: Value = serde_json::from_str(&stdout(&output)).unwrap();

    assert_eq!(
        config["update_call_sites"],
        serde_json::json!(["includes", "templates"])
    );
    assert_eq!(config["parallel"], 4);
}

#[test]
fn the_printed_configuration_is_a_valid_strauss_json() {
    // `strauss-rs config > strauss.json` must produce a file that means the same thing.
    let first = stdout(&strauss(&["config", "-C", &path("test/config-fixture")]));

    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("round-trip");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("strauss.json"), &first).unwrap();
    std::fs::copy(
        path("test/config-fixture/composer.json"),
        directory.join("composer.json"),
    )
    .unwrap();

    let second = stdout(&strauss(&["config", "-C", &directory.to_string_lossy()]));

    std::fs::remove_dir_all(&directory).unwrap();

    assert_eq!(second, first);
}

#[test]
fn uses_the_current_directory_by_default() {
    let output = Command::new(env!("CARGO_BIN_EXE_strauss-rs"))
        .arg("config")
        .current_dir(path("test/config-fixture"))
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));

    let config: Value = serde_json::from_str(&stdout(&output)).unwrap();

    assert_eq!(config["target_directory"], "lib/prefixed");
}

#[test]
fn a_directory_without_configuration_uses_the_defaults() {
    let output = strauss(&["config", "-C", &path("test")]);

    assert!(output.status.success(), "{}", stderr(&output));

    let config: Value = serde_json::from_str(&stdout(&output)).unwrap();

    assert_eq!(config["target_directory"], "vendor-prefixed");
}

#[test]
fn reports_a_missing_project_directory() {
    let output = strauss(&["config", "-C", &path("test/does-not-exist")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("error: could not open project directory"));
    assert!(stdout(&output).is_empty());
}

#[test]
fn reports_an_invalid_configuration() {
    let output = strauss(&["config", "-C", &path("test/config-fixture-invalid")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).starts_with("error: strauss.json: unknown field `target_dir`"),
        "{}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty());
}

#[test]
fn reports_an_invalid_option_value_as_a_usage_error() {
    let output = strauss(&["config", "--parallel=lots"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("expected `auto`, `true`, `false`, or a whole number"));
}

#[test]
fn analyses_a_php_file() {
    let output = strauss(&["analyse", &path("test/TestPhp.php")]);

    assert!(output.status.success(), "{}", stderr(&output));

    let analysis = stdout(&output);

    assert!(analysis.contains("FileAnalysis"));
    assert!(analysis.contains("kind: FunctionName"));
}

#[test]
fn reports_an_unreadable_php_file() {
    let output = strauss(&["analyse", &path("test/missing.php")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("error: could not read"));
}

#[test]
fn shows_usage_without_a_command() {
    let output = strauss(&[]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Usage: strauss-rs [OPTIONS] <COMMAND>"));
}

#[test]
fn reports_its_version() {
    let output = strauss(&["--version"]);

    assert!(output.status.success());
    assert_eq!(
        stdout(&output).trim(),
        format!("strauss-rs {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn parallel_is_auto_unless_set() {
    let default = strauss(&["config", "-C", &path("test/config-fixture")]);
    let explicit = strauss(&[
        "config",
        "-C",
        &path("test/config-fixture"),
        "--parallel=auto",
    ]);
    let off = strauss(&[
        "config",
        "-C",
        &path("test/config-fixture"),
        "--parallel=false",
    ]);

    let parallel = |output: &Output| -> Value {
        assert!(output.status.success(), "{}", stderr(output));

        serde_json::from_str::<Value>(&stdout(output)).unwrap()["parallel"].clone()
    };

    assert_eq!(parallel(&default), "auto");
    assert_eq!(parallel(&explicit), "auto");
    assert_eq!(parallel(&off), false);
    // Spelling it out changes nothing.
    assert_eq!(default.stdout, explicit.stdout);
}

#[test]
fn list_symbols_takes_types_comma_separated_or_repeated() {
    let fixture = path("test/symbols-fixture/vendor");

    let comma = strauss(&["list-symbols", &fixture, "-t", "class,function"]);
    let repeated = strauss(&["list-symbols", &fixture, "--type=class", "--type=FUNCTION"]);

    assert!(comma.status.success(), "{}", stderr(&comma));
    assert_eq!(stdout(&comma), stdout(&repeated));

    let text = stdout(&comma);

    assert!(text.contains("class      GlobalClass"), "{text}");
    assert!(text.contains("function   global_function"), "{text}");
    assert!(!text.contains("namespace "), "{text}");

    // The unparseable fixture file is reported, not fatal.
    assert!(stderr(&comma).contains("broken.php"));
}

#[test]
fn list_symbols_global_with_one_type_prints_only_names() {
    let output = strauss(&[
        "list-symbols",
        &path("test/symbols-fixture/vendor"),
        "--global",
        "-t",
        "function",
    ]);

    assert!(output.status.success(), "{}", stderr(&output));

    for line in stdout(&output).lines() {
        assert!(!line.contains(' ') && !line.contains('\\'), "{line}");
    }

    assert!(
        stdout(&output)
            .lines()
            .any(|line| line == "global_function")
    );
}

#[test]
fn list_symbols_rejects_an_unknown_type() {
    let output = strauss(&["list-symbols", "-t", "bogus"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("possible values"));
}
