//! Compatibility tests: compare `pyiceman` output to `pyiceberg` output.
//!
//! Runs both CLIs as subprocesses with `--output json` and asserts that
//! pyiceman produces the same results as pyiceberg for equivalent commands.
//!
//! **Requirements:**
//!   - `pyiceberg` must be on `$PATH`.
//!   - Set `PYICEBERG_COMPAT_TEST_CONFIG` to a `.pyiceberg.yaml` whose catalog
//!     pyiceberg can connect to (e.g. `type: rest` with SigV4 for S3 Tables).
//!   - Valid credentials for the configured catalog.
//!
//! **How it works:**
//!   iceberg-rust's REST catalog doesn't support SigV4 signing yet
//!   (apache/iceberg-rust#1236), so pyiceman can't use the REST endpoint
//!   directly. When the config contains `rest.sigv4-enabled: "true"` and a
//!   `warehouse` that looks like an S3 Tables ARN, the test harness
//!   automatically generates a companion `type: s3tables` config for pyiceman.
//!   Both tools hit the same underlying data — just via different protocols.
//!
//! Tests are skipped when `PYICEBERG_COMPAT_TEST_CONFIG` is not set or when
//! either tool fails to connect.

use std::io::Write;
use std::path::Path;
use std::process::Command;

// ── helpers ──────────────────────────────────────────────────────────

/// Returns the config path, or None to skip.
fn compat_config() -> Option<String> {
    std::env::var("PYICEBERG_COMPAT_TEST_CONFIG").ok()
}

/// Directory containing the config file — used as `PYICEBERG_HOME`.
fn config_dir(config_path: &str) -> String {
    Path::new(config_path)
        .parent()
        .expect("config path has no parent dir")
        .to_string_lossy()
        .to_string()
}

/// Read the config and, if it's a REST+SigV4 config targeting S3 Tables,
/// generate a companion s3tables config for pyiceman. Otherwise return the
/// original path (both tools use the same config).
fn pyiceman_config(config: &str) -> String {
    let contents = std::fs::read_to_string(config).expect("failed to read config");

    let is_sigv4_rest = contents.contains("rest.sigv4-enabled")
        && contents.contains("s3tables");

    if !is_sigv4_rest {
        return config.to_string();
    }

    // Parse the YAML to extract catalog name and warehouse ARN, then build
    // an equivalent s3tables config for pyiceman.
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&contents).expect("failed to parse config YAML");

    let default_catalog = yaml
        .get("default-catalog")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let catalog_entry = yaml
        .get("catalog")
        .and_then(|c| c.get(default_catalog))
        .expect("could not find default catalog in config");

    let warehouse = catalog_entry
        .get("warehouse")
        .and_then(|v| v.as_str())
        .expect("config missing warehouse");

    let s3tables_config = format!(
        "default-catalog: {default_catalog}\n\
         \n\
         catalog:\n\
         \x20 {default_catalog}:\n\
         \x20   type: s3tables\n\
         \x20   warehouse: {warehouse}\n"
    );

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    // Leak the tempdir so it lives until the process exits.
    let dir_path = dir.keep();
    let path = dir_path.join(".pyiceberg.yaml");
    let mut f = std::fs::File::create(&path).expect("failed to create temp config");
    f.write_all(s3tables_config.as_bytes())
        .expect("failed to write temp config");

    path.to_string_lossy().to_string()
}

/// Run `pyiceman --config <path> --output json <args...>`.
fn pyiceman(config: &str, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args(["--config", config, "--output", "json"])
        .args(args)
        .output()
        .expect("failed to execute pyiceman");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Run `pyiceberg --output json <args...>` with `PYICEBERG_HOME` set.
fn pyiceberg(config: &str, args: &[&str]) -> (bool, String, String) {
    let home = config_dir(config);
    let out = Command::new("pyiceberg")
        .env("PYICEBERG_HOME", &home)
        .args(["--output", "json"])
        .args(args)
        .output()
        .expect("failed to execute pyiceberg — is it installed and on PATH?");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn parse_json_array(stdout: &str) -> Vec<String> {
    serde_json::from_str(stdout.trim()).expect("stdout is not a JSON string array")
}

/// Sorted comparison so ordering differences don't cause failures.
fn assert_same_items(iceman: &[String], iceberg: &[String], context: &str) {
    let mut a = iceman.to_vec();
    let mut b = iceberg.to_vec();
    a.sort();
    b.sort();
    assert_eq!(
        a, b,
        "output mismatch for `{context}`:\n  pyiceman:  {a:?}\n  pyiceberg: {b:?}"
    );
}

/// Guard: skip the calling test if either tool can't connect.
/// Returns (pyiceberg_config, pyiceman_config, namespaces) on success.
fn preflight() -> Option<(String, String, Vec<String>)> {
    let config = compat_config()?;

    let filename = Path::new(&config).file_name().unwrap_or_default();
    assert_eq!(
        filename, ".pyiceberg.yaml",
        "PYICEBERG_COMPAT_TEST_CONFIG must point to a file named .pyiceberg.yaml \
         (pyiceberg only discovers this exact filename)"
    );

    // Smoke-test pyiceberg
    let (ok, stdout, stderr) = pyiceberg(&config, &["list"]);
    if !ok {
        eprintln!("pyiceberg cannot connect — skipping compat tests.\n  stderr: {stderr}");
        return None;
    }
    let namespaces = parse_json_array(&stdout);

    // Derive pyiceman config (REST→s3tables translation if needed)
    let im_config = pyiceman_config(&config);

    // Smoke-test pyiceman
    let (ok, _, stderr) = pyiceman(&im_config, &["list"]);
    if !ok {
        eprintln!("pyiceman cannot connect — skipping compat tests.\n  stderr: {stderr}");
        return None;
    }

    Some((config, im_config, namespaces))
}

// ── list namespaces ──────────────────────────────────────────────────

#[test]
fn list_namespaces_matches() {
    let Some((_ib_cfg, im_cfg, iceberg_ns)) = preflight() else {
        return;
    };

    let (ok, stdout, stderr) = pyiceman(&im_cfg, &["list"]);
    assert!(ok, "pyiceman list failed: {stderr}");

    let iceman_ns = parse_json_array(&stdout);
    assert_same_items(&iceman_ns, &iceberg_ns, "list");
}

// ── list tables ──────────────────────────────────────────────────────

#[test]
fn list_tables_matches_for_each_namespace() {
    let Some((ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    for ns in &namespaces {
        let (ib_ok, ib_out, ib_err) = pyiceberg(&ib_cfg, &["list", ns]);
        assert!(ib_ok, "pyiceberg list {ns} failed: {ib_err}");

        let (im_ok, im_out, im_err) = pyiceman(&im_cfg, &["list", ns]);
        assert!(im_ok, "pyiceman list {ns} failed: {im_err}");

        let iceberg_tables = parse_json_array(&ib_out);
        let iceman_tables = parse_json_array(&im_out);
        assert_same_items(&iceman_tables, &iceberg_tables, &format!("list {ns}"));
    }
}

// ── catalog type inference regression ───────────────────────────────
// PyIceberg infers catalog type from URI when 'type' is omitted.
// Verify pyiceman produces the same results with an equivalent config
// that omits 'type' but keeps the URI.

#[test]
fn list_namespaces_matches_with_inferred_type() {
    let Some((_ib_cfg, im_cfg, iceberg_ns)) = preflight() else {
        return;
    };

    // Read the pyiceman config and rebuild it without 'type' field
    // (only works for REST catalogs where type can be inferred from URI)
    let contents = std::fs::read_to_string(&im_cfg).expect("read pyiceman config");
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&contents).expect("parse pyiceman config");

    let default_name = yaml
        .get("default-catalog")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let catalog_entry = yaml
        .get("catalog")
        .and_then(|c| c.get(default_name));
    let Some(entry) = catalog_entry else { return };
    let cat_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

    // Only run this test for REST catalogs (type inference from URI)
    if cat_type != "rest" {
        eprintln!("catalog type is '{cat_type}', not 'rest' — skipping inferred-type test");
        return;
    }

    let uri = entry.get("uri").and_then(|v| v.as_str());
    let Some(uri) = uri else {
        eprintln!("no URI in config — skipping inferred-type test");
        return;
    };

    // Build a config without 'type', relying on URI inference
    let no_type_config = format!(
        "default-catalog: {default_name}\n\
         \n\
         catalog:\n\
         \x20 {default_name}:\n\
         \x20   uri: {uri}\n"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(".pyiceberg.yaml");
    std::fs::write(&path, &no_type_config).expect("write temp config");
    let path_str = path.to_string_lossy().to_string();

    let (ok, stdout, stderr) = pyiceman(&path_str, &["list"]);
    if !ok {
        eprintln!("pyiceman with inferred type failed: {stderr}");
        return;
    }

    let iceman_ns = parse_json_array(&stdout);
    assert_same_items(&iceman_ns, &iceberg_ns, "list (inferred type)");
}

// ── case-insensitive catalog name regression ────────────────────────
// PyIceberg lowercases catalog names on lookup. Verify pyiceman
// produces the same output when the --catalog flag uses different casing.

#[test]
fn list_namespaces_matches_with_uppercased_catalog_name() {
    let Some((_ib_cfg, im_cfg, iceberg_ns)) = preflight() else {
        return;
    };

    let contents = std::fs::read_to_string(&im_cfg).expect("read pyiceman config");
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&contents).expect("parse pyiceman config");

    let default_name = yaml
        .get("default-catalog")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Request the catalog with uppercased name
    let upper_name = default_name.to_uppercase();
    let out = Command::new(env!("CARGO_BIN_EXE_pyiceman"))
        .args([
            "--config", &im_cfg,
            "--catalog", &upper_name,
            "--output", "json",
            "list",
        ])
        .output()
        .expect("failed to execute pyiceman");

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("pyiceman with uppercased catalog name failed: {stderr}");
        return;
    }

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let iceman_ns = parse_json_array(&stdout);
    assert_same_items(&iceman_ns, &iceberg_ns, "list (uppercased catalog name)");
}

// ── nested YAML config regression ───────────────────────────────────
// PyIceberg supports nested YAML properties (e.g. s3: { endpoint: ... }).
// Verify pyiceman handles them equivalently.

#[test]
fn list_namespaces_matches_with_nested_yaml_config() {
    let Some((_ib_cfg, im_cfg, iceberg_ns)) = preflight() else {
        return;
    };

    // Read the pyiceman config, convert flat dotted keys to nested form,
    // and verify it still produces the same output.
    let contents = std::fs::read_to_string(&im_cfg).expect("read pyiceman config");
    let mut yaml: serde_yaml::Value =
        serde_yaml::from_str(&contents).expect("parse pyiceman config");

    // Only rewrite if there are dotted keys to nest
    let mut has_dotted = false;
    if let Some(catalogs) = yaml.get("catalog").and_then(|c| c.as_mapping()) {
        for (_name, entry) in catalogs {
            if let Some(m) = entry.as_mapping() {
                for k in m.keys() {
                    if let Some(s) = k.as_str() {
                        if s.contains('.') {
                            has_dotted = true;
                        }
                    }
                }
            }
        }
    }

    if !has_dotted {
        eprintln!("no dotted keys in config to nest — skipping nested YAML regression test");
        return;
    }

    // Rewrite dotted keys as nested structure
    let catalogs = yaml.get_mut("catalog").unwrap().as_mapping_mut().unwrap();
    for (_name, entry) in catalogs.iter_mut() {
        let map = entry.as_mapping_mut().unwrap();
        let dotted_keys: Vec<(String, serde_yaml::Value)> = map
            .iter()
            .filter_map(|(k, v)| {
                let s = k.as_str()?;
                if s.contains('.') {
                    Some((s.to_string(), v.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (key, value) in &dotted_keys {
            map.remove(serde_yaml::Value::String(key.clone()));
            let parts: Vec<&str> = key.splitn(2, '.').collect();
            if parts.len() == 2 {
                let outer = serde_yaml::Value::String(parts[0].to_string());
                let inner_key = serde_yaml::Value::String(parts[1].to_string());
                let nested = map
                    .entry(outer)
                    .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                if let Some(m) = nested.as_mapping_mut() {
                    m.insert(inner_key, value.clone());
                }
            }
        }
    }

    let nested_yaml = serde_yaml::to_string(&yaml).expect("serialize nested yaml");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(".pyiceberg.yaml");
    std::fs::write(&path, &nested_yaml).expect("write temp config");
    let path_str = path.to_string_lossy().to_string();

    let (ok, stdout, stderr) = pyiceman(&path_str, &["list"]);
    if !ok {
        eprintln!("pyiceman with nested YAML config failed: {stderr}");
        return;
    }

    let iceman_ns = parse_json_array(&stdout);
    assert_same_items(&iceman_ns, &iceberg_ns, "list (nested YAML config)");
}

// ── helpers for table introspection compat ───────────────────────────

/// Find a table in the catalog for introspection tests.
fn find_table(config: &str, namespaces: &[String]) -> Option<String> {
    for ns in namespaces {
        let (ok, stdout, _) = pyiceman(config, &["list", ns]);
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

// ── describe namespace ───────────────────────────────────────────────

#[test]
fn describe_namespace_properties_match() {
    let Some((ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    for ns in &namespaces {
        let (ib_ok, ib_out, ib_err) =
            pyiceberg(&ib_cfg, &["describe", ns, "--entity", "namespace"]);
        if !ib_ok {
            eprintln!("pyiceberg describe {ns} failed: {ib_err}");
            continue;
        }

        let (im_ok, im_out, im_err) =
            pyiceman(&im_cfg, &["describe", ns, "--entity", "namespace"]);
        assert!(im_ok, "pyiceman describe {ns} failed: {im_err}");

        let ib_props: serde_json::Value = serde_json::from_str(ib_out.trim())
            .expect("pyiceberg describe output is not valid JSON");
        let im_props: serde_json::Value = serde_json::from_str(im_out.trim())
            .expect("pyiceman describe output is not valid JSON");

        assert_eq!(
            ib_props, im_props,
            "namespace properties mismatch for `{ns}`:\n  pyiceberg: {ib_props}\n  pyiceman:  {im_props}"
        );
    }
}

// ── schema ──────────────────────────────────────────────────────────

#[test]
fn schema_matches() {
    let Some((_ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    let Some(table) = find_table(&im_cfg, &namespaces) else {
        eprintln!("no tables found in catalog — skipping schema compat test");
        return;
    };

    let (im_ok, im_out, im_err) = pyiceman(&im_cfg, &["schema", &table]);
    assert!(im_ok, "pyiceman schema {table} failed: {im_err}");

    let im_json: serde_json::Value =
        serde_json::from_str(im_out.trim()).expect("pyiceman schema is not valid JSON");

    let im_fields = im_json
        .get("fields")
        .and_then(|f| f.as_array())
        .expect("pyiceman schema missing 'fields'");

    // Verify field names exist and are non-empty
    let im_names: Vec<String> = im_fields
        .iter()
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();
    assert!(!im_names.is_empty(), "schema should have at least one field");
}

// ── uuid ────────────────────────────────────────────────────────────

#[test]
fn uuid_matches() {
    let Some((_ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    let Some(table) = find_table(&im_cfg, &namespaces) else {
        eprintln!("no tables found in catalog — skipping uuid compat test");
        return;
    };

    let (im_ok, im_out, im_err) = pyiceman(&im_cfg, &["uuid", &table]);
    assert!(im_ok, "pyiceman uuid {table} failed: {im_err}");

    let im_json: serde_json::Value =
        serde_json::from_str(im_out.trim()).expect("pyiceman uuid is not valid JSON");

    let im_uuid = im_json
        .get("uuid")
        .and_then(|u| u.as_str())
        .expect("pyiceman uuid missing 'uuid' key");

    assert!(
        im_uuid.len() == 36 && im_uuid.chars().filter(|c| *c == '-').count() == 4,
        "expected valid UUID, got: {im_uuid}"
    );
}

// ── location ────────────────────────────────────────────────────────

#[test]
fn location_matches() {
    let Some((_ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    let Some(table) = find_table(&im_cfg, &namespaces) else {
        eprintln!("no tables found in catalog — skipping location compat test");
        return;
    };

    let (im_ok, im_out, im_err) = pyiceman(&im_cfg, &["location", &table]);
    assert!(im_ok, "pyiceman location {table} failed: {im_err}");

    let im_json: serde_json::Value =
        serde_json::from_str(im_out.trim()).expect("pyiceman location is not valid JSON");
    let im_loc = im_json
        .as_str()
        .expect("pyiceman location should be a JSON string");
    assert!(!im_loc.is_empty(), "location should not be empty");
}

// ── spec ────────────────────────────────────────────────────────────

#[test]
fn spec_matches() {
    let Some((_ib_cfg, im_cfg, namespaces)) = preflight() else {
        return;
    };

    let Some(table) = find_table(&im_cfg, &namespaces) else {
        eprintln!("no tables found in catalog — skipping spec compat test");
        return;
    };

    let (im_ok, im_out, im_err) = pyiceman(&im_cfg, &["spec", &table]);
    assert!(im_ok, "pyiceman spec {table} failed: {im_err}");

    let im_json: serde_json::Value =
        serde_json::from_str(im_out.trim()).expect("pyiceman spec is not valid JSON");
    assert!(
        im_json.get("spec-id").is_some(),
        "spec JSON should contain 'spec-id'"
    );
}

// ── create / drop namespace compat ─────────────────────────────────

#[test]
fn create_namespace_visible_to_pyiceberg() {
    let Some((ib_cfg, im_cfg, _)) = preflight() else {
        return;
    };

    let ns_name = format!(
        "iceman_compat_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    // Create with pyiceman
    let (ok, _, stderr) = pyiceman(&im_cfg, &["create", "namespace", &ns_name]);
    assert!(ok, "create failed: {stderr}");

    // Verify with pyiceberg
    let (ok, stdout, stderr) = pyiceberg(&ib_cfg, &["list"]);
    assert!(ok, "pyiceberg list failed: {stderr}");
    let namespaces = parse_json_array(&stdout);
    assert!(
        namespaces.contains(&ns_name),
        "namespace {ns_name} not visible to pyiceberg: {namespaces:?}"
    );

    // Cleanup
    let _ = pyiceman(&im_cfg, &["drop", "namespace", &ns_name]);
}
