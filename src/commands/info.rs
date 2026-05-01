use std::io::{self, Write};

use anyhow::{Result, bail};
use serde::Serialize;

use crate::cli::OutputFormat;

const METADATA_MD: &str = include_str!("../../assets/info/metadata.md");
const SCHEMAS_MD: &str = include_str!("../../assets/info/schemas.md");

#[derive(Clone, Copy)]
enum Source {
    Metadata,
    Schemas,
}

struct Topic {
    name: &'static str,
    aliases: &'static [&'static str],
    blurb: &'static str,
    source: Source,
    sections: &'static [&'static str],
}

const TOPICS: &[Topic] = &[
    Topic {
        name: "metadata",
        aliases: &["tables", "overview", "metadata-tables"],
        blurb: "Inventory of Iceberg metadata tables and a quick task-to-table reference.",
        source: Source::Metadata,
        sections: &["Metadata Tables", "Quick Reference by Task"],
    },
    Topic {
        name: "status-codes",
        aliases: &["status"],
        blurb: "Status codes used in entries / all_entries.",
        source: Source::Metadata,
        sections: &["Status Codes"],
    },
    Topic {
        name: "content-types",
        aliases: &["content"],
        blurb: "Manifest content type codes (data vs delete).",
        source: Source::Metadata,
        sections: &["Content Type Codes"],
    },
    Topic {
        name: "history",
        aliases: &[],
        blurb: "Schema for the history metadata table (commit ancestry).",
        source: Source::Schemas,
        sections: &["history"],
    },
    Topic {
        name: "metadata-log",
        aliases: &["metadata_log_entries", "metadata-log-entries"],
        blurb: "Schema for metadata_log_entries (metadata.json file generations).",
        source: Source::Schemas,
        sections: &["metadata_log_entries"],
    },
    Topic {
        name: "snapshots",
        aliases: &[],
        blurb: "Schema for the snapshots metadata table.",
        source: Source::Schemas,
        sections: &["snapshots"],
    },
    Topic {
        name: "entries",
        aliases: &["all-entries", "all_entries"],
        blurb: "Schema for entries / all_entries (per-file operations).",
        source: Source::Schemas,
        sections: &["entries"],
    },
    Topic {
        name: "files",
        aliases: &["data-files", "delete-files"],
        blurb: "Schema for the files table (active data + delete files).",
        source: Source::Schemas,
        sections: &["files"],
    },
    Topic {
        name: "manifests",
        aliases: &["all-manifests", "all_manifests"],
        blurb: "Schema for manifests / all_manifests.",
        source: Source::Schemas,
        sections: &["manifests"],
    },
    Topic {
        name: "partitions",
        aliases: &[],
        blurb: "Schema for the partitions metadata table (per-partition stats).",
        source: Source::Schemas,
        sections: &["partitions"],
    },
    Topic {
        name: "all-data-files",
        aliases: &["all-delete-files", "all_data_files", "all_delete_files"],
        blurb: "Schema for all_data_files / all_delete_files (cross-snapshot files).",
        source: Source::Schemas,
        sections: &["all_data_files"],
    },
    Topic {
        name: "refs",
        aliases: &[],
        blurb: "Schema for the refs metadata table (branches and tags).",
        source: Source::Schemas,
        sections: &["refs"],
    },
];

fn normalize(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace('_', "-")
}

fn find_topic(query: &str) -> Option<&'static Topic> {
    let q = normalize(query);
    TOPICS.iter().find(|t| {
        normalize(t.name) == q || t.aliases.iter().any(|a| normalize(a) == q)
    })
}

fn source_text(src: Source) -> &'static str {
    match src {
        Source::Metadata => METADATA_MD,
        Source::Schemas => SCHEMAS_MD,
    }
}

/// Extract a `## <heading>` section from `md`, returning it including the heading.
/// The section ends at the next H2 (or H1) or EOF, with any trailing `---`
/// separator stripped.
fn extract_section(md: &str, heading: &str) -> Option<String> {
    let needle = format!("\n## {heading}\n");
    let start = md.find(&needle)?;
    let body_start = start + 1; // skip the leading newline so output begins with `## `
    let after = &md[body_start + needle.len() - 1..];
    // Find the next section boundary: a line starting with "## " or "# ".
    let end_rel = find_next_heading(after);
    let block = match end_rel {
        Some(idx) => &md[body_start..body_start + (needle.len() - 1) + idx],
        None => &md[body_start..],
    };
    Some(strip_trailing_separator(block).to_string())
}

/// Strip the trailing `\n---\n` divider (and surrounding whitespace) that
/// schemas.md uses between sections, without nibbling legitimate hyphens.
fn strip_trailing_separator(block: &str) -> &str {
    let trimmed = block.trim_end();
    if let Some(prefix) = trimmed.strip_suffix("\n---") {
        prefix.trim_end()
    } else if trimmed == "---" {
        ""
    } else {
        trimmed
    }
}

fn find_next_heading(s: &str) -> Option<usize> {
    let mut offset = 0;
    for line in s.split_inclusive('\n') {
        if (line.starts_with("## ") || line.starts_with("# "))
            && !line.starts_with("### ")
            && offset != 0
        {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

fn render_topic(topic: &Topic) -> Result<String> {
    let md = source_text(topic.source);
    let mut out = String::new();
    for (i, heading) in topic.sections.iter().enumerate() {
        let section = extract_section(md, heading).ok_or_else(|| {
            anyhow::anyhow!(
                "internal: section '{heading}' not found in embedded source for topic '{}'",
                topic.name
            )
        })?;
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&section);
    }
    out.push('\n');
    Ok(out)
}

#[derive(Serialize)]
struct TopicJson<'a> {
    topic: &'a str,
    content: String,
}

#[derive(Serialize)]
struct TopicListJson<'a> {
    topics: Vec<TopicEntryJson<'a>>,
}

#[derive(Serialize)]
struct TopicEntryJson<'a> {
    name: &'a str,
    aliases: &'a [&'a str],
    description: &'a str,
}

pub fn run(topic: Option<&str>, fmt: OutputFormat) -> Result<()> {
    match topic {
        None => list_topics(fmt),
        Some(name) => {
            let Some(t) = find_topic(name) else {
                let mut msg = format!("unknown topic '{name}'. available topics:\n");
                for entry in TOPICS {
                    msg.push_str("  ");
                    msg.push_str(entry.name);
                    msg.push('\n');
                }
                bail!("{}", msg.trim_end());
            };
            let content = render_topic(t)?;
            let mut out = io::stdout().lock();
            match fmt {
                OutputFormat::Json => {
                    let payload = TopicJson { topic: t.name, content };
                    serde_json::to_writer(&mut out, &payload)?;
                    out.write_all(b"\n")?;
                }
                OutputFormat::Text => {
                    out.write_all(content.as_bytes())?;
                }
            }
            Ok(())
        }
    }
}

fn list_topics(fmt: OutputFormat) -> Result<()> {
    let mut out = io::stdout().lock();
    match fmt {
        OutputFormat::Json => {
            let payload = TopicListJson {
                topics: TOPICS
                    .iter()
                    .map(|t| TopicEntryJson {
                        name: t.name,
                        aliases: t.aliases,
                        description: t.blurb,
                    })
                    .collect(),
            };
            serde_json::to_writer(&mut out, &payload)?;
            out.write_all(b"\n")?;
        }
        OutputFormat::Text => {
            let name_width = TOPICS.iter().map(|t| t.name.len()).max().unwrap_or(0);
            writeln!(out, "available topics:")?;
            for t in TOPICS {
                writeln!(out, "  {:width$}  {}", t.name, t.blurb, width = name_width)?;
            }
            writeln!(out)?;
            writeln!(out, "usage: iceman info <topic>")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_resolves_and_renders() {
        for t in TOPICS {
            let rendered = render_topic(t)
                .unwrap_or_else(|e| panic!("topic {} failed: {e}", t.name));
            assert!(!rendered.trim().is_empty(), "topic {} rendered empty", t.name);
            assert!(
                rendered.starts_with("## "),
                "topic {} did not start with '## ': {rendered:?}",
                t.name
            );
        }
    }

    #[test]
    fn aliases_resolve_to_same_topic() {
        for t in TOPICS {
            for alias in t.aliases {
                let resolved = find_topic(alias).unwrap_or_else(|| panic!("alias {alias} not found"));
                assert_eq!(resolved.name, t.name);
            }
        }
    }

    #[test]
    fn lookup_is_case_and_separator_insensitive() {
        assert_eq!(find_topic("Partitions").map(|t| t.name), Some("partitions"));
        assert_eq!(find_topic("ALL_ENTRIES").map(|t| t.name), Some("entries"));
        assert_eq!(find_topic("metadata_log_entries").map(|t| t.name), Some("metadata-log"));
    }

    #[test]
    fn unknown_topic_errors_with_list() {
        let err = run(Some("nope"), OutputFormat::Text).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown topic 'nope'"));
        assert!(msg.contains("partitions"));
    }

    #[test]
    fn refs_section_extracts_to_eof_cleanly() {
        let s = extract_section(SCHEMAS_MD, "refs").unwrap();
        assert!(s.starts_with("## refs"));
        assert!(s.contains("max_reference_age_in_ms"));
        assert!(!s.ends_with("---"));
    }

    #[test]
    fn metadata_topic_concatenates_two_sections() {
        let t = find_topic("metadata").unwrap();
        let rendered = render_topic(t).unwrap();
        assert!(rendered.contains("## Metadata Tables"));
        assert!(rendered.contains("## Quick Reference by Task"));
    }
}
