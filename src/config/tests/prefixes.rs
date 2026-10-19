use super::*;

#[test]
fn trims_slashes_from_target_directory() {
    assert_eq!(
        strauss_only(r#"{"target_directory": "/lib/prefixed/"}"#).target_directory,
        "lib/prefixed"
    );
}

#[test]
fn rejects_target_directory_outside_the_project() {
    for directory in ["", "/", "../out", "a/../../out", "C:\\out", "."] {
        let json = format!(
            r#"{{"target_directory": {}}}"#,
            serde_json::to_string(directory).unwrap()
        );
        let message = error(&json);

        assert!(
            message.contains("`target_directory`"),
            "{directory:?}: {message}"
        );
    }
}

#[test]
fn rejects_invalid_prefixes() {
    assert!(error(r#"{"namespace_prefix": ""}"#).contains("must not be empty"));
    assert!(error(r#"{"namespace_prefix": "\\"}"#).contains("must not be empty"));
    assert!(
        error(r#"{"namespace_prefix": "My-Plugin\\Lib"}"#)
            .contains("`My-Plugin` is not a valid name")
    );
    assert!(error(r#"{"namespace_prefix": "A\\\\B"}"#).contains("is not a valid name"));
    assert!(error(r#"{"classmap_prefix": ""}"#).contains("must not be empty"));
    assert!(error(r#"{"classmap_prefix": "My Plugin_"}"#).contains("can only contain letters"));
    assert!(error(r#"{"classmap_prefix": "1st_"}"#).contains("can't start with a number"));
    assert!(error(r#"{"constant_prefix": "A-B_"}"#).contains("can only contain letters"));
}

#[test]
fn a_namespace_is_not_a_classmap_prefix() {
    let message = error(r#"{"classmap_prefix": "SoaringLeads\\Lefan_"}"#);

    assert!(message.contains("can only contain letters"), "{message}");
    assert!(message.contains("`namespace_prefix`"), "{message}");

    // The hint is for `classmap_prefix` only.
    assert!(!error(r#"{"constant_prefix": "A\\B_"}"#).contains("`namespace_prefix`"));
}

#[test]
fn empty_constant_prefix_leaves_constants_alone() {
    assert_eq!(
        strauss_only(r#"{"constant_prefix": ""}"#).constant_prefix,
        None
    );
}

#[test]
fn trims_backslashes_from_namespace_prefix() {
    assert_eq!(
        strauss_only(r#"{"namespace_prefix": "\\Acme\\Plugin\\"}"#)
            .namespace_prefix
            .as_deref(),
        Some("Acme\\Plugin")
    );
}

#[test]
fn functions_prefix_defaults_to_the_lowercase_classmap_prefix() {
    let config = strauss_only(r#"{"classmap_prefix": "Acme_Plugin_"}"#);

    assert_eq!(config.functions_prefix.as_deref(), Some("acme_plugin_"));
}

#[test]
fn functions_prefix_can_be_set_or_disabled() {
    let set = strauss_only(r#"{"classmap_prefix": "Acme_", "functions_prefix": "acme_fn_"}"#);
    let alias = strauss_only(r#"{"function_prefix": "acme_fn_"}"#);
    let disabled_false = strauss_only(r#"{"classmap_prefix": "Acme_", "functions_prefix": false}"#);
    let disabled_empty = strauss_only(r#"{"classmap_prefix": "Acme_", "functions_prefix": ""}"#);
    let null = strauss_only(r#"{"classmap_prefix": "Acme_", "functions_prefix": null}"#);

    assert_eq!(set.functions_prefix.as_deref(), Some("acme_fn_"));
    assert_eq!(alias.functions_prefix.as_deref(), Some("acme_fn_"));
    assert_eq!(disabled_false.functions_prefix, None);
    assert_eq!(disabled_empty.functions_prefix, None);
    // As in Strauss, `null` is the same as leaving it out.
    assert_eq!(null.functions_prefix.as_deref(), Some("acme_"));
}

#[test]
fn rejects_invalid_functions_prefix() {
    assert!(error(r#"{"functions_prefix": true}"#).contains("or false to disable"));
    assert!(error(r#"{"functions_prefix": 3}"#).contains("or false to disable"));
    assert!(error(r#"{"functions_prefix": "a-b"}"#).contains("can only contain letters"));
}
