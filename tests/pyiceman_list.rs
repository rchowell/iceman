//! Integration tests for `pyiceman list`.
//!
//! These tests run the `pyiceman` binary as a subprocess with `--output json`
//! and assert on the parsed JSON output.
//!
//! **Requirements:**
//!   - Set `PYICEMAN_TEST_CONFIG` to a `.pyiceberg.yaml` path with valid catalog credentials.
//!   - AWS credentials must be available in the environment.
//!
//! Tests are skipped automatically when `PYICEMAN_TEST_CONFIG` is not set.

use std::process::Command;

fn test_config() -> Option<String> {
    std::env::var("PYICEMAN_TEST_CONFIG").ok()
}

/// Run `pyiceman` with the test config, `--output json`, and the given args.
/// Returns (exit_status_success, stdout, stderr).
fn pyiceman(args: &[&str]) -> (bool, String, String) {
    let config = test_config().expect("PYICEMAN_TEST_CONFIG must be set");
    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args(["--config", &config, "--output", "json"])
        .args(args)
        .output()
        .expect("failed to execute pyiceman");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn parse_json_array(stdout: &str) -> Vec<String> {
    serde_json::from_str(stdout.trim()).expect("stdout is not a JSON string array")
}

// ── list namespaces ──────────────────────────────────────────────────

#[test]
fn list_root_namespaces() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, stdout, stderr) = pyiceman(&["list"]);
    assert!(ok, "pyiceman list failed: {stderr}");

    let namespaces = parse_json_array(&stdout);
    assert!(!namespaces.is_empty(), "expected at least one namespace");
}

#[test]
fn list_namespaces_returns_valid_json_array() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, stdout, _) = pyiceman(&["list"]);
    assert!(ok);

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    assert!(parsed.is_array(), "expected JSON array, got: {parsed}");
}

// ── list tables ──────────────────────────────────────────────────────

#[test]
fn list_tables_in_namespace() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    // First, get a namespace to query
    let (ok, stdout, stderr) = pyiceman(&["list"]);
    assert!(ok, "pyiceman list failed: {stderr}");

    let namespaces = parse_json_array(&stdout);
    assert!(!namespaces.is_empty(), "need at least one namespace");

    let ns = &namespaces[0];
    let (ok, stdout, stderr) = pyiceman(&["list", ns]);
    assert!(ok, "pyiceman list {ns} failed: {stderr}");

    let tables = parse_json_array(&stdout);
    // Tables may be empty, but the output must be valid JSON
    for table in &tables {
        assert!(
            table.contains('.'),
            "table identifier should be namespace-qualified: {table}"
        );
    }
}

// ── error cases ──────────────────────────────────────────────────────

#[test]
fn list_nonexistent_namespace_fails() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, _, _) = pyiceman(&["list", "nonexistent_ns_that_should_not_exist_12345"]);
    assert!(!ok, "expected failure for nonexistent namespace");
}

// ── version (no catalog needed) ──────────────────────────────────────

#[test]
fn version_prints_semver() {
    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .arg("version")
        .output()
        .expect("failed to execute pyiceman");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("pyiceman "),
        "expected 'pyiceman <version>', got: {stdout}"
    );
}

// ── catalog-less operation (no config entry) ────────────────────────

#[test]
fn uri_flag_without_config_entry_does_not_error_catalog_not_found() {
    // Passing --uri without a catalog entry in config should NOT fail with
    // "catalog not found". It will fail to connect (no server), but the
    // error should be a connection error, not a config error.
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(".pyiceberg.yaml");
    std::fs::write(&config_path, "# empty config\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "--uri",
            "http://localhost:19999",
            "list",
        ])
        .output()
        .expect("failed to execute pyiceman");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not contain "catalog.*not found"
    assert!(
        !stderr.to_lowercase().contains("not found in configuration"),
        "got catalog-not-found error when using --uri flag: {stderr}"
    );
}

#[test]
fn catalog_type_inferred_from_uri() {
    // When type is omitted but --uri is http, should infer REST (and fail
    // to connect, not fail with "no catalog type").
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(".pyiceberg.yaml");
    std::fs::write(
        &config_path,
        "catalog:\n  default:\n    uri: http://localhost:19999\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "list",
        ])
        .output()
        .expect("failed to execute pyiceman");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("no catalog type specified"),
        "type should be inferred from URI, got: {stderr}"
    );
}

#[test]
fn config_show_handles_nested_yaml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(".pyiceberg.yaml");
    std::fs::write(
        &config_path,
        r#"
catalog:
  my_cat:
    type: rest
    uri: http://localhost:8181
    s3:
      endpoint: http://localhost:9000
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args(["--config", config_path.to_str().unwrap(), "config", "show"])
        .output()
        .expect("failed to execute pyiceman");

    assert!(
        output.status.success(),
        "config show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn case_insensitive_catalog_name_via_cli() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    // Read the config to find the default catalog name, then request it
    // in uppercase. Should still work.
    let config = test_config().unwrap();
    let contents = std::fs::read_to_string(&config).unwrap();
    let yaml: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();

    let catalog_name = yaml
        .get("default-catalog")
        .and_then(|v| v.as_str())
        .or_else(|| {
            yaml.get("catalog")
                .and_then(|c| c.as_mapping())
                .and_then(|m| m.keys().next())
                .and_then(|k| k.as_str())
        });

    let Some(name) = catalog_name else {
        eprintln!("could not determine catalog name from config — skipping");
        return;
    };

    let upper_name = name.to_uppercase();
    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args([
            "--config",
            &config,
            "--catalog",
            &upper_name,
            "--output",
            "json",
            "list",
        ])
        .output()
        .expect("failed to execute pyiceman");

    assert!(
        output.status.success(),
        "case-insensitive catalog lookup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── describe ─────────────────────────────────────────────────────────

fn pyiceman_text(args: &[&str]) -> (bool, String, String) {
    let config = test_config().expect("PYICEMAN_TEST_CONFIG must be set");
    let output = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args(["--config", &config, "--output", "text"])
        .args(args)
        .output()
        .expect("failed to execute pyiceman");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn describe_namespace_text_output() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, stdout, stderr) = pyiceman(&["list"]);
    assert!(ok, "pyiceman list failed: {stderr}");
    let namespaces = parse_json_array(&stdout);
    assert!(!namespaces.is_empty(), "need at least one namespace");

    let ns = &namespaces[0];
    let (ok, stdout, stderr) = pyiceman_text(&["describe", ns, "--entity", "namespace"]);
    assert!(ok, "pyiceman describe namespace failed: {stderr}");
    assert!(
        !stdout.trim().is_empty(),
        "expected non-empty text output for describe namespace"
    );
}

#[test]
fn describe_nonexistent_fails() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, _, stderr) = pyiceman(&[
        "describe",
        "nonexistent_ns_that_should_not_exist_12345",
        "--entity",
        "namespace",
    ]);
    assert!(
        !ok,
        "expected failure for nonexistent namespace, stderr: {stderr}"
    );
}

#[test]
fn describe_namespace_json_output() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let (ok, stdout, stderr) = pyiceman(&["list"]);
    assert!(ok, "pyiceman list failed: {stderr}");
    let namespaces = parse_json_array(&stdout);
    assert!(!namespaces.is_empty(), "need at least one namespace");

    let ns = &namespaces[0];
    let (ok, stdout, stderr) = pyiceman(&["describe", ns, "--entity", "namespace"]);
    assert!(
        ok,
        "pyiceman describe namespace --output json failed: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    assert!(parsed.is_object(), "expected JSON object, got: {parsed}");
}

// ── table introspection ─────────────────────────────────────────────

/// Find a table to use for introspection tests.
fn find_any_table() -> Option<String> {
    let (ok, stdout, _) = pyiceman(&["list"]);
    if !ok {
        return None;
    }
    let namespaces = parse_json_array(&stdout);
    for ns in &namespaces {
        let (ok, stdout, _) = pyiceman(&["list", ns]);
        if !ok {
            continue;
        }
        let tables = parse_json_array(&stdout);
        if let Some(table) = tables.first() {
            return Some(table.clone());
        }
    }
    None
}

#[test]
fn schema_prints_fields() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let Some(table) = find_any_table() else {
        eprintln!("no tables found in catalog — skipping");
        return;
    };

    let (ok, stdout, stderr) = pyiceman(&["schema", &table]);
    assert!(ok, "pyiceman schema {table} failed: {stderr}");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    assert!(
        parsed.get("fields").is_some(),
        "schema JSON should contain 'fields' key, got: {parsed}"
    );
    let fields = parsed["fields"].as_array().expect("fields should be array");
    assert!(!fields.is_empty(), "schema should have at least one field");
}

#[test]
fn uuid_returns_valid_uuid() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let Some(table) = find_any_table() else {
        eprintln!("no tables found in catalog — skipping");
        return;
    };

    let (ok, stdout, stderr) = pyiceman(&["uuid", &table]);
    assert!(ok, "pyiceman uuid {table} failed: {stderr}");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    let uuid_str = parsed["uuid"]
        .as_str()
        .expect("uuid JSON should have 'uuid' string key");

    assert!(
        uuid_str.len() == 36 && uuid_str.chars().filter(|c| *c == '-').count() == 4,
        "expected valid UUID format, got: {uuid_str}"
    );
}

#[test]
fn location_returns_string() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let Some(table) = find_any_table() else {
        eprintln!("no tables found in catalog — skipping");
        return;
    };

    let (ok, stdout, stderr) = pyiceman(&["location", &table]);
    assert!(ok, "pyiceman location {table} failed: {stderr}");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    assert!(parsed.is_string(), "expected JSON string, got: {parsed}");
    assert!(!parsed.as_str().unwrap().is_empty(), "location should not be empty");
}

#[test]
fn spec_returns_json() {
    if test_config().is_none() {
        eprintln!("PYICEMAN_TEST_CONFIG not set — skipping");
        return;
    }

    let Some(table) = find_any_table() else {
        eprintln!("no tables found in catalog — skipping");
        return;
    };

    let (ok, stdout, stderr) = pyiceman(&["spec", &table]);
    assert!(ok, "pyiceman spec {table} failed: {stderr}");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    assert!(
        parsed.get("spec-id").is_some(),
        "spec JSON should contain 'spec-id', got: {parsed}"
    );
}
