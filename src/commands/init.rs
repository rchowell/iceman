use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::catalog::default_config_path;

const DEFAULT_CONFIG: &str = "\
# iceman configuration
#
# default-catalog = \"local\"
#
# [catalog.local]
# type = \"rest\"
# uri = \"http://localhost:8181\"
# warehouse = \"my_warehouse\"
";

pub fn run(path: Option<&Path>, force: bool) -> Result<()> {
    let target: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    if target.exists() && !force {
        bail!(
            "config already exists at {}; pass --force to overwrite",
            target.display()
        );
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    std::fs::write(&target, DEFAULT_CONFIG)
        .with_context(|| format!("writing {}", target.display()))?;

    println!("wrote {}", target.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_config_to_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nested").join("config.toml");

        run(Some(&target), false).unwrap();

        assert!(target.exists());
        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.contains("default-catalog"));
    }

    #[test]
    fn init_refuses_existing_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        std::fs::write(&target, "existing = true").unwrap();

        let err = run(Some(&target), false).unwrap_err();
        assert!(err.to_string().contains("--force"));

        let contents = std::fs::read_to_string(&target).unwrap();
        assert_eq!(contents, "existing = true");
    }

    #[test]
    fn init_force_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        std::fs::write(&target, "stale = true").unwrap();

        run(Some(&target), true).unwrap();

        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.contains("default-catalog"));
        assert!(!contents.contains("stale"));
    }
}
