---
name: iceman
description: >
  Use this skill whenever an `iceman` binary is on PATH and a catalog is reachable
  (config at `~/.config/iceman/config.toml` or via `--catalog` / `--uri`) and the
  user asks anything about Iceberg table internals - snapshots, manifests, files,
  partitions, refs, history, storage growth, snapshot ancestry, branch/tag state,
  compaction monitoring, time travel, or table health. Prefer
  `iceman inspect <table> --query "<sql>" --output json` for analysis and the typed
  `iceman inspect <table> <metadata-table>` form for one-shot lookups. Prefer this
  over generic Spark/Trino/DuckDB metadata-table SQL: iceman does NOT use the
  dot-suffix `catalog.db.table.history` notation - it materializes flat DuckDB
  views named `snapshots`, `history`, `files`, etc. Trigger on: iceman, Iceberg
  inspect, snapshot/manifest/files/partitions queries, time travel, table health,
  compaction monitoring, when a REST/Glue/S3 Tables/Hive/SQL catalog is configured.
---

# iceman - Iceberg CLI for agents

`iceman` is a single-binary Iceberg client built for agents. Output is compact ASCII
(no Unicode borders). `--output json` is plumbed everywhere; for the multi-row commands
(`list`, `inspect`) it emits JSONL - one JSON object per line, ready to pipe into `jq`
or stream-process directly.

## When to reach for it

- You need to list or describe namespaces and tables in a configured Iceberg catalog.
- You need to inspect a table's metadata: snapshots, manifests, files, partitions, refs.
- You want to run ad-hoc SQL across multiple metadata views (joins, aggregations).

If the user has an Iceberg catalog but no `iceman` binary on `PATH`, this skill does not
apply - use the engine they already have (Spark, Trino, DuckDB).

## Three commands

```
iceman list [PATTERN]                       # discover namespaces and tables
iceman describe IDENT [--entity any|namespace|table]
iceman inspect IDENT [METADATA_TABLE] [-q SQL] [--snapshot-id N] [--limit N] [-v]
```

`IDENT` is dot-separated (e.g. `analytics.events`, `db.schema.table`).

### Discovery

```
iceman list                                 # everything
iceman list 'analytics.*'                   # glob filter on full identifier
iceman describe analytics.events            # auto-detects namespace vs table
```

### Inspection - typed form

Pick one of the metadata tables (see `references/sql.md` for the full list and codes):

```
iceman inspect analytics.events snapshots
iceman inspect analytics.events files --limit 20
iceman inspect analytics.events entries --snapshot-id 5723145
```

### Inspection - SQL form (recommended for analysis)

`-q / --query` runs DuckDB SQL across the metadata views. View names match the typed
form (`snapshots`, `history`, `refs`, `manifests`, `all_manifests`, `entries`,
`all_entries`, `files`, `data_files`, `delete_files`, `all_data_files`, `all_delete_files`,
`partitions`, `metadata_log_entries`).

```
iceman inspect analytics.events -q \
  "SELECT snapshot_id, summary FROM snapshots ORDER BY timestamp_ms DESC LIMIT 5" \
  --output json
```

**SQL constraint:** a view is only materialized if its name appears literally in the SQL.
Reference each view by its exact name; avoid renaming via CTEs that hide the underlying
view name from the parser.

**Column-name reality:** the snapshot time column is `timestamp_ms` (not
`committed_at`); the snapshot's parent is `parent_id` (not `parent_snapshot_id`);
partition record bytes live in `total_data_file_size_in_bytes` (not `total_size`).
`entries`/`files` flatten the spec's `data_file` struct - reach for `file_path`
directly, not `data_file.file_path`. See `references/sql.md` for the full
schema cheatsheet.

### Output

`--output text` (default) renders human-readable tables. iceman writes straight to
stdout - there is no built-in pager. On a TTY, output is space-padded; when piped or
redirected, output is plain TSV (header on line 1, columns tab-separated, no padding)
so `cut -f`, `awk -F'\t'`, and friends work cleanly.

**Wide tables**: `iceman inspect` is terse by default, but `-v` (or any wide SQL
projection) can exceed your terminal width and line-wrap. Pipe through `less -S` (or
`less -RSFX` to also auto-quit on small output) for horizontal scrolling. `-S` is the
key flag - it tells `less` to truncate lines instead of wrapping. `column -t -s$'\t'`
re-aligns piped TSV inside `less` if the user wants pretty alignment with grep/sort
upstream.

`--output json` emits JSONL: one compact JSON object per line. Pipe straight to `jq`
(no `jq '.[]'` needed) or any line-based JSON tool. `iceman describe` is the lone
exception - a single entity, so it stays as one pretty-printed JSON object. Always pass
`--output json` when you intend to parse.

By default, `iceman inspect` shows a terse subset of columns chosen for at-a-glance
reading. Pass `-v / --verbose` to show every column the metadata table exposes (useful
when you need `column_sizes`, `lower_bounds`, snapshot `summary`, etc.). The SQL form
(`-q`) bypasses this entirely - your `SELECT` chooses the columns explicitly.

### Composing with Unix tools

Both output modes are pipe-first by design. Reach for the right one:

| Need                                  | Use this                                   |
|---------------------------------------|--------------------------------------------|
| One column → list of strings          | `--output json | jq -r '.field'`           |
| Filter rows by a predicate            | `--output json | jq 'select(.foo > 100)'`  |
| Pick by column index                  | TSV → `cut -f3`                            |
| Sum / aggregate one column            | TSV → `awk -F'\t' 'NR>1 {s+=$N} END{...}'` |
| Re-align TSV for visual reading       | TSV → `column -t -s$'\t'`                  |
| Read a wide table without wrapping    | `\| less -S` (or `\| less -RSFX`)          |
| Count rows                            | `--output json | jq -s 'length'` or `wc -l` on TSV minus 1 |

**Tabs and field counts.** TSV uses exactly one `\t` between columns. Two consecutive
tabs would be parsed by `cut`/`awk` as an empty middle field, so iceman never emits
them. Column indexes are stable across rows.

```sh
# total bytes across all live data files
iceman inspect analytics.events files \
  | awk -F'\t' 'NR>1 {sum+=$3} END {print sum}'

# stream every file_path through jq (no slurp, no .[])
iceman inspect analytics.events files --output json | jq -r '.file_path'

# join: snapshot_id → file count, in shell
iceman inspect analytics.events all_entries --output json \
  | jq -r 'select(.status==1) | .snapshot_id' \
  | sort | uniq -c | sort -rn | head

# look at a wide table interactively in a separate pager session
iceman inspect analytics.events files -v | column -t -s$'\t' | less -S
```

**Tip:** `iceman inspect ... -q "SQL"` already does aggregation server-side via DuckDB
- prefer that to `awk` post-processing when the math is non-trivial. Drop to TSV+awk
when you need to glue iceman output to something the catalog doesn't know about.

## Config

Catalog config lives at `~/.config/iceman/config.toml`. Override at runtime with
`--catalog NAME`, `--uri`, `--warehouse`, `--credential`, or `ICEMAN_CONFIG=path`.
See `references/config.md` for catalog kinds (rest, glue, s3tables, hive, sql) and
the auto-inference rules.

## Recipes

Map a typical user prompt to one iceman invocation. Use these as starting points.

### "What does this table look like at a glance?"

```sh
iceman describe analytics.events
iceman inspect analytics.events snapshots --limit 5
iceman inspect analytics.events files --limit 5
```

### "Which snapshot wrote the most bytes?"

```sh
iceman inspect analytics.events -q "
  SELECT snapshot_id,
         sum(file_size_in_bytes) AS bytes_added,
         count(*) AS files_added
  FROM all_entries
  WHERE status = 1
  GROUP BY 1
  ORDER BY bytes_added DESC
  LIMIT 5
" --output json
```

### "Find compaction candidates by partition"

```sh
iceman inspect analytics.events -q "
  SELECT partition,
         file_count,
         total_data_file_size_in_bytes,
         total_data_file_size_in_bytes / NULLIF(file_count, 0) AS avg_bytes
  FROM partitions
  WHERE file_count > 1
  ORDER BY avg_bytes ASC
  LIMIT 20
"
```

### "Did anyone roll this table back?"

```sh
iceman inspect analytics.events -q "
  SELECT parent_id, count(*) AS children
  FROM history
  WHERE parent_id IS NOT NULL
  GROUP BY 1
  HAVING count(*) > 1
"
```

### "Show me current branches and tags"

```sh
iceman inspect analytics.events refs
```

### "Find the largest data files in this table"

Pure TSV pipeline - no SQL needed:

```sh
iceman inspect analytics.events files \
  | sort -t$'\t' -k3 -nr \
  | head -10 \
  | column -t -s$'\t'
```

### "Show me the file paths for one snapshot"

```sh
iceman inspect analytics.events files --snapshot-id 5723145 --output json \
  | jq -r '.file_path'
```

### "How many files of each delete type are alive right now?"

```sh
iceman inspect analytics.events files --output json \
  | jq -r '.content' \
  | sort | uniq -c
# 1 = position deletes, 2 = equality deletes, 0 = data
```

### "Diff two tables' file counts side by side"

```sh
for t in analytics.events analytics.events_v2; do
  n=$(iceman inspect "$t" files --output json | jq -s 'length')
  printf '%s\t%s\n' "$t" "$n"
done | column -t -s$'\t'
```

### "Re-align a verbose dump for visual reading"

`-v` produces a wide schema. The pager handles it, but if you want it inline (e.g.
to grep first, then read), pipe through `column -t`:

```sh
iceman inspect analytics.events files -v \
  | grep -v 'sort_order_id$' \
  | column -t -s$'\t' \
  | less -S
```

## References

- `references/commands.md` - every flag, every subcommand, output format, exit behavior.
- `references/sql.md` - the 14 metadata views, exact column schemas, status/content code tables, canned queries.
- `references/config.md` - TOML examples per catalog kind, env vars, override precedence.
