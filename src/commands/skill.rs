use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const SKILL_NAME: &str = "iceman";

const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../assets/skill/SKILL.md"),
    ),
    (
        "references/commands.md",
        include_str!("../../assets/skill/references/commands.md"),
    ),
    (
        "references/sql.md",
        include_str!("../../assets/skill/references/sql.md"),
    ),
    (
        "references/config.md",
        include_str!("../../assets/skill/references/config.md"),
    ),
];

pub fn install(location: Option<&Path>, user: bool, force: bool) -> Result<()> {
    let parent = resolve_parent(location, user)?;
    let target = parent.join(SKILL_NAME);

    if target.exists() {
        if !force {
            bail!(
                "skill already installed at {}; pass --force to overwrite",
                target.display()
            );
        }
        std::fs::remove_dir_all(&target)
            .with_context(|| format!("removing existing {}", target.display()))?;
    }

    std::fs::create_dir_all(target.join("references"))
        .with_context(|| format!("creating {}", target.display()))?;

    for (rel, contents) in SKILL_FILES {
        let path = target.join(rel);
        std::fs::write(&path, contents)
            .with_context(|| format!("writing {}", path.display()))?;
        println!("wrote {}", path.display());
    }

    println!("installed iceman skill at {}", target.display());
    Ok(())
}

fn resolve_parent(location: Option<&Path>, user: bool) -> Result<PathBuf> {
    if user {
        let home = dirs::home_dir().context("could not determine home directory")?;
        return Ok(home.join(".claude").join("skills"));
    }
    if let Some(p) = location {
        return Ok(p.to_path_buf());
    }
    Ok(PathBuf::from("./.claude/skills"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_writes_skill_files() {
        let dir = tempfile::tempdir().unwrap();
        install(Some(dir.path()), false, false).unwrap();

        let target = dir.path().join("iceman");
        assert!(target.exists());

        let skill_md = std::fs::read_to_string(target.join("SKILL.md")).unwrap();
        assert!(skill_md.starts_with("---\n"));

        for (rel, _) in SKILL_FILES {
            assert!(target.join(rel).exists(), "missing {rel}");
        }
    }

    #[test]
    fn install_refuses_existing_without_force() {
        let dir = tempfile::tempdir().unwrap();
        install(Some(dir.path()), false, false).unwrap();
        let err = install(Some(dir.path()), false, false).unwrap_err();
        assert!(err.to_string().contains("--force"));
    }

    #[test]
    fn install_force_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        install(Some(dir.path()), false, false).unwrap();

        let stale = dir.path().join("iceman").join("references").join("stale.md");
        std::fs::write(&stale, "delete me").unwrap();
        assert!(stale.exists());

        install(Some(dir.path()), false, true).unwrap();
        assert!(!stale.exists());
        assert!(dir.path().join("iceman").join("SKILL.md").exists());
    }
}
