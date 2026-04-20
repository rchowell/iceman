//! Integration tests for `pyiceman create` and `pyiceman drop`.
//!
//! Uses a SQLite-backed catalog for full isolation -- no remote service needed.

mod common;

use std::process::Command;

/// Run pyiceman with a given config and args.
/// Returns (exit_status_success, stdout, stderr).
fn pyiceman(config: &str, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args(["--config", config])
        .args(args)
        .output()
        .expect("failed to execute pyiceman");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[tokio::test]
async fn create_and_list_namespace() {
    let tc = common::TestCatalog::new().await;
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = tc.write_pyiceberg_config(&config_dir);
    let config_str = config_path.to_str().unwrap();

    // Create namespace
    let (ok, stdout, stderr) = pyiceman(config_str, &["create", "namespace", "test_ns"]);
    assert!(ok, "create namespace failed: {stderr}");
    assert!(
        stdout.contains("Created namespace"),
        "unexpected output: {stdout}"
    );

    // List should now include it
    let (ok, stdout, stderr) = pyiceman(config_str, &["--output", "json", "list"]);
    assert!(ok, "list failed: {stderr}");
    let namespaces: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert!(
        namespaces.contains(&"test_ns".to_string()),
        "namespace not found: {namespaces:?}"
    );
}

#[tokio::test]
async fn drop_namespace() {
    let tc = common::TestCatalog::new().await;
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = tc.write_pyiceberg_config(&config_dir);
    let config_str = config_path.to_str().unwrap();

    // Create, then drop
    pyiceman(config_str, &["create", "namespace", "to_drop"]);
    let (ok, stdout, stderr) = pyiceman(config_str, &["drop", "namespace", "to_drop"]);
    assert!(ok, "drop namespace failed: {stderr}");
    assert!(
        stdout.contains("Dropped namespace"),
        "unexpected output: {stdout}"
    );

    // Should no longer appear
    let (ok, stdout, _) = pyiceman(config_str, &["--output", "json", "list"]);
    assert!(ok);
    let namespaces: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert!(!namespaces.contains(&"to_drop".to_string()));
}

#[tokio::test]
async fn drop_nonexistent_namespace_fails() {
    let tc = common::TestCatalog::new().await;
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = tc.write_pyiceberg_config(&config_dir);
    let config_str = config_path.to_str().unwrap();

    let (ok, _, _) = pyiceman(config_str, &["drop", "namespace", "nonexistent"]);
    assert!(!ok, "expected failure for nonexistent namespace");
}

#[tokio::test]
async fn drop_nonexistent_table_fails() {
    let tc = common::TestCatalog::new().await;
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = tc.write_pyiceberg_config(&config_dir);
    let config_str = config_path.to_str().unwrap();

    let (ok, _, _) = pyiceman(config_str, &["drop", "table", "ns.nonexistent"]);
    assert!(!ok, "expected failure for nonexistent table");
}
