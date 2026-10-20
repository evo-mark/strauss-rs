use super::*;

#[test]
fn reads_update_call_sites() {
    assert_eq!(
        strauss_only(r#"{"update_call_sites": false}"#).update_call_sites,
        UpdateCallSites::Disabled
    );
    assert_eq!(
        strauss_only(r#"{"update_call_sites": true}"#).update_call_sites,
        UpdateCallSites::FromAutoload
    );
    assert_eq!(
        strauss_only(r#"{"update_call_sites": []}"#).update_call_sites,
        UpdateCallSites::Disabled
    );
    assert_eq!(
        strauss_only(r#"{"update_call_sites": ["includes", "templates"]}"#).update_call_sites,
        UpdateCallSites::Paths(vec!["includes".into(), "templates".into()])
    );
}

#[test]
fn rejects_invalid_update_call_sites() {
    assert!(error(r#"{"update_call_sites": "src"}"#).contains("true, false, or an array"));
    assert!(
        error(r#"{"update_call_sites": [1]}"#).contains("`update_call_sites[0]` must be a string")
    );
    assert!(error(r#"{"update_call_sites": [""]}"#).contains("must not be empty"));
}

#[test]
fn reads_parallel() {
    assert_eq!(
        strauss_only(r#"{"parallel": true}"#).parallel,
        Parallel::Auto
    );
    assert_eq!(strauss_only(r#"{"parallel": 0}"#).parallel, Parallel::Auto);
    assert_eq!(
        strauss_only(r#"{"parallel": false}"#).parallel,
        Parallel::Disabled
    );
    assert_eq!(
        strauss_only(r#"{"parallel": 1}"#).parallel,
        Parallel::Disabled
    );
    assert_eq!(
        strauss_only(r#"{"parallel": 4}"#).parallel,
        Parallel::Workers(4)
    );
}

#[test]
fn rejects_invalid_parallel() {
    assert!(error(r#"{"parallel": -1}"#).contains("true, false, or a whole number"));
    assert!(error(r#"{"parallel": 1.5}"#).contains("true, false, or a whole number"));
    assert!(error(r#"{"parallel": "4"}"#).contains("true, false, or a whole number"));
}

#[test]
fn keeps_namespace_replacement_patterns_in_file_order() {
    let config = strauss_only(
        r#"{"namespace_replacement_patterns": {
                "Zebra": "First",
                "~^Apple\\\\(.*)~": "Second\\\\$1",
                "Middle": "Third"
            }}"#,
    );

    let patterns: Vec<&str> = config
        .namespace_replacement_patterns
        .iter()
        .map(|entry| entry.pattern.source())
        .collect();

    assert_eq!(patterns, ["Zebra", "~^Apple\\\\(.*)~", "Middle"]);
}

#[test]
fn namespace_replacement_patterns_can_be_applied() {
    let config = strauss_only(
        r#"{"namespace_replacement_patterns": {
                "~^BrianHenryIE\\\\(.*)~": "BrianHenryIE\\\\WC_Cash_App_Gateway\\\\$1"
            }}"#,
    );

    let entry = &config.namespace_replacement_patterns[0];

    assert_eq!(
        entry
            .pattern
            .replace_all("BrianHenryIE\\Lib", &entry.replacement)
            .unwrap(),
        "BrianHenryIE\\WC_Cash_App_Gateway\\Lib"
    );
}

#[test]
fn rejects_invalid_namespace_replacement_patterns() {
    assert!(
        error(r#"{"namespace_replacement_patterns": {"~(~": "x"}}"#)
            .contains("`namespace_replacement_patterns`")
    );
    assert!(
        error(r#"{"namespace_replacement_patterns": {"": "x"}}"#).contains("must not be empty")
    );
    assert!(error(r#"{"namespace_replacement_patterns": {"A": 1}}"#).contains("invalid type"));
}

#[test]
fn reads_override_autoload() {
    let config = strauss_only(
        r#"{"override_autoload": {
                "acme/lib": {"psr-4": {"Acme\\Lib\\": ["src/", "lib/"]}, "classmap": ["extra/"], "files": ["f.php"]}
            }}"#,
    );

    let autoload = &config.override_autoload["acme/lib"];

    assert_eq!(autoload.psr4.0[0].0, "Acme\\Lib\\");
    assert_eq!(
        autoload.psr4.0[0].1.iter().collect::<Vec<_>>(),
        ["src/", "lib/"]
    );
    assert_eq!(autoload.classmap, ["extra/"]);
    assert_eq!(autoload.files, ["f.php"]);
}

#[test]
fn reads_boolean_options() {
    let config = strauss_only(
        r#"{
                "include_root_autoload": true,
                "optimize_autoloader": false,
                "delete_vendor_packages": true,
                "delete_vendor_files": true,
                "exclude_git_files": false,
                "include_modified_date": false,
                "include_author": false
            }"#,
    );

    assert!(config.include_root_autoload);
    assert!(!config.optimize_autoloader);
    assert!(config.delete_vendor_packages);
    assert!(config.delete_vendor_files);
    assert!(!config.exclude_git_files);
    assert!(!config.include_modified_date);
    assert!(!config.include_author);
}

#[test]
fn rejects_non_boolean_booleans() {
    assert!(error(r#"{"delete_vendor_packages": "yes"}"#).contains("invalid type: string"));
    assert!(error(r#"{"optimize_autoloader": 1}"#).contains("invalid type"));
}

#[test]
fn parses_the_command_line_form_of_update_call_sites() {
    assert_eq!("true".parse(), Ok(UpdateCallSites::FromAutoload));
    assert_eq!("TRUE".parse(), Ok(UpdateCallSites::FromAutoload));
    assert_eq!("false".parse(), Ok(UpdateCallSites::Disabled));
    assert_eq!(
        "includes, templates ,".parse(),
        Ok(UpdateCallSites::Paths(vec![
            "includes".into(),
            "templates".into()
        ]))
    );
    assert_eq!(
        "src".parse(),
        Ok(UpdateCallSites::Paths(vec!["src".into()]))
    );
    assert!("".parse::<UpdateCallSites>().is_err());
    assert!(" , ".parse::<UpdateCallSites>().is_err());
}

#[test]
fn parses_the_command_line_form_of_parallel() {
    assert_eq!("true".parse(), Ok(Parallel::Auto));
    assert_eq!("false".parse(), Ok(Parallel::Disabled));
    assert_eq!("0".parse(), Ok(Parallel::Auto));
    assert_eq!("1".parse(), Ok(Parallel::Disabled));
    assert_eq!("6".parse(), Ok(Parallel::Workers(6)));
    assert_eq!(" 6 ".parse(), Ok(Parallel::Workers(6)));
    assert!("-1".parse::<Parallel>().is_err());
    assert!("many".parse::<Parallel>().is_err());
    assert!("1.5".parse::<Parallel>().is_err());
}

#[test]
fn parallel_defaults_to_auto() {
    assert_eq!(strauss_only("{}").parallel, Parallel::Auto);
    assert_eq!(
        strauss_only(r#"{"parallel": null}"#).parallel,
        Parallel::Auto
    );
}

#[test]
fn parallel_accepts_auto_in_any_case() {
    for value in ["auto", "AUTO", "Auto"] {
        assert_eq!(
            strauss_only(&format!(r#"{{"parallel": "{value}"}}"#)).parallel,
            Parallel::Auto,
            "{value}"
        );
    }
}

#[test]
fn auto_true_and_zero_mean_the_same() {
    for value in [r#""auto""#, "true", "0"] {
        assert_eq!(
            strauss_only(&format!(r#"{{"parallel": {value}}}"#)).parallel,
            Parallel::Auto,
            "{value}"
        );
    }
}

#[test]
fn parallel_rejects_other_strings_and_names_auto() {
    for value in [r#""fast""#, r#""4""#, r#""""#, r#""autos""#] {
        let message = error(&format!(r#"{{"parallel": {value}}}"#));

        assert!(
            message.contains(r#"`parallel` must be "auto", true, false, or a whole number"#),
            "{value}: {message}"
        );
    }
}

#[test]
fn parses_auto_on_the_command_line() {
    assert_eq!("auto".parse(), Ok(Parallel::Auto));
    assert_eq!("AUTO".parse(), Ok(Parallel::Auto));
    assert_eq!(" auto ".parse(), Ok(Parallel::Auto));

    let message = "autos".parse::<Parallel>().unwrap_err();

    assert_eq!(
        message,
        "expected `auto`, `true`, `false`, or a whole number"
    );
}

#[test]
fn packages_default_to_require_only() {
    let composer = r#"{"require": {"php": "^8", "a/a": "^1"}, "require-dev": {"b/b": "^1"}}"#;

    assert_eq!(config("{}", composer).packages, vec!["php", "a/a"]);
}

#[test]
fn packages_includes_dev_adds_require_dev() {
    let composer =
        r#"{"require": {"a/a": "^1", "c/c": "^1"}, "require-dev": {"b/b": "^1", "c/c": "^1"}}"#;

    assert_eq!(
        config(r#"{"packages_includes_dev": true}"#, composer).packages,
        vec!["a/a", "c/c", "b/b"]
    );
}

#[test]
fn packages_includes_dev_is_ignored_when_packages_are_listed() {
    let composer = r#"{"require": {"a/a": "^1"}, "require-dev": {"b/b": "^1"}}"#;

    assert_eq!(
        config(
            r#"{"packages": ["a/a"], "packages_includes_dev": true}"#,
            composer
        )
        .packages,
        vec!["a/a"]
    );
}
