# Iceman

An Apache Iceberg CLI built in Rust, designed for AI agents.

Powered by [iceberg-rust](https://github.com/apache/iceberg-rust).

## Install

```sh
cargo install --path .
```

## Configure

```sh
mkdir -p ~/.config/iceman
$EDITOR ~/.config/iceman/config.toml
```

```toml
default-catalog = "local"

[catalog.local]
type = "rest"
uri = "http://localhost:8181"
warehouse = "my_warehouse"
s3.endpoint = "http://localhost:9000"
```

Override at runtime with `--catalog NAME`, `--uri`, `--warehouse`, `--credential`,
or `ICEMAN_CONFIG=path`.

## Commands

```
iceman list [PATTERN]                                # discover namespaces and tables
iceman describe IDENT [--entity any|namespace|table] # describe namespace or table
iceman inspect IDENT [METADATA_TABLE] [-q SQL] [-v]  # inspect table internals
iceman skill install                                 # install the bundled Claude skill
iceman version
```

## Output

`iceman` writes straight to stdout. No built-in pager, no terminal trickery — every
command behaves the same. Output adapts to where it's going:

- **TTY**: columns are space-padded and aligned. Wide tables (especially `iceman inspect
  -v`) will line-wrap; pipe through `less -S` (or `less -RSFX`) to scroll horizontally
  instead.
- **Pipe / redirect** → tab-separated values: header on line 1, no padding, single
  `\t` between columns. `cut -f`, `awk -F'\t'`, and friends parse it directly.
- **`--output json`** → JSONL (one compact JSON object per line) for `iceman list` and
  `iceman inspect`. Pipe straight to `jq` — no `jq '.[]'` needed. `iceman describe`
  describes a single entity, so it stays as one pretty-printed JSON object.

By default, `iceman inspect` shows a terse subset of columns chosen for at-a-glance
reading. Pass `-v / --verbose` to show every column. The SQL form (`-q`) chooses
columns explicitly via `SELECT`, so `-v` doesn't apply.

**Reading wide tables.** When a table is too wide for your terminal:

```sh
iceman inspect analytics.events files -v | less -S          # scroll left/right
iceman inspect analytics.events files -v | less -RSFX       # quit if it fits
iceman inspect analytics.events files -v | column -t -s$'\t' | less -S  # re-aligned
```

`-S` is the key flag — it tells `less` to truncate long lines instead of wrapping.

## Examples

### Discover what's in the catalog

```sh
iceman list
iceman list 'analytics.*'
iceman describe analytics.events
```

### Inspect a table — terse by default

```sh
iceman inspect analytics.events snapshots --limit 5
iceman inspect analytics.events files --limit 5
iceman inspect analytics.events partitions
iceman inspect analytics.events refs
```

`files` shows `content, record_count, file_size_in_bytes, file_path` by default —
the wide `column_sizes`, `lower_bounds`, `upper_bounds` columns appear with `-v`:

```sh
iceman inspect analytics.events files --limit 5 -v
```

### Pipe to standard tools

When piped, output is plain TSV — header on line 1, tabs between columns:

```sh
# total bytes across all data files
iceman inspect analytics.events files \
  | awk -F'\t' 'NR>1 {sum+=$3} END {print sum}'

# extract just the file paths
iceman inspect analytics.events files | cut -f4 | tail -n +2

# re-align the TSV for visual reading in another tool
iceman inspect analytics.events files | column -t -s$'\t' | less -S
```

### JSONL → jq

```sh
# count rows
iceman inspect analytics.events files --output json | jq -s 'length'

# stream one field per line — no .[] needed
iceman inspect analytics.events files --output json | jq -r '.file_path'

# filter and reshape
iceman inspect analytics.events snapshots --output json \
  | jq -r 'select(.operation == "append") | "\(.snapshot_id)\t\(.timestamp_ms)"'
```

### Ad-hoc SQL across the metadata views

`iceman inspect IDENT -q SQL` runs DuckDB SQL across flat views named after the
metadata tables (`snapshots`, `history`, `refs`, `manifests`, `entries`, `files`,
`data_files`, `delete_files`, `partitions`, etc.). A view is materialized only if
its name appears literally in the SQL.

```sh
# bytes added per snapshot
iceman inspect analytics.events -q "
  SELECT snapshot_id,
         sum(file_size_in_bytes) AS bytes_added,
         count(*) AS files_added
  FROM all_entries
  WHERE status = 1
  GROUP BY 1
  ORDER BY bytes_added DESC
  LIMIT 10
"

# compaction candidates by partition (small files)
iceman inspect analytics.events -q "
  SELECT partition,
         file_count,
         total_data_file_size_in_bytes,
         total_data_file_size_in_bytes / NULLIF(file_count, 0) AS avg_bytes
  FROM partitions
  WHERE file_count > 1
  ORDER BY avg_bytes ASC
  LIMIT 20
" --output json
```

## Use with Claude Code

Iceman ships with a Claude skill that teaches the agent when and how to reach for
each command:

```sh
iceman skill install            # ./.claude/skills/iceman/
iceman skill install --user     # ~/.claude/skills/iceman/
```

## License

Apache-2.0
