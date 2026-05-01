# Iceman

Iceman is a tool for working with Apache Iceberg. 

> I made this tool for introspecting Iceberg metadata tables. It is intentionally designed
> to play well with unix tools like less/cut/awk so that you (or Claude) can effectively
> use tool outputs. If you have duckdb installed, then you can inspect with '-q' and you
> (or Claude) can run SQL queries against the metadata tables which is quite nice.

## Install

```sh
# Homebrew
brew install rchowell/tap/iceman

# Or from source
cargo install --path .
```

## Agents

Iceman ships with a Claude skill that teaches the agent how to use it.

```sh
iceman skill install            # ./.claude/skills/iceman/
iceman skill install --user     # ~/.claude/skills/iceman/
```

Iceman also has an `info` command which writes markdown to provide additonal
context for agents. This is like MCP resources and has helped me a lot.

```sh
iceman info metadata    # prints info about all metadata tables
iceman info partitions  # prints info about the 'partitions' table
```

## Usage

These are the basic commands, and you can always use `--help` to learn more.

```
iceman list [PATTERN]                                # discover namespaces and tables
iceman describe IDENT [--entity any|namespace|table] # describe namespace or table
iceman inspect IDENT [METADATA_TABLE] [-q SQL] [-v]  # inspect table internals
iceman info TERM                                     # print info about iceberg things
iceman skill install                                 # install the bundled Claude skill
iceman version
```

Configuration is stored in ~/.config/iceman as TOML.

```sh
# Creates ~/.config/iceman/config.toml
iceman init
```

```toml
default-catalog = "default"

[catalog.default]
type = "sql"
uri = "sqlite:////path/to/catalog.sqlite3"
warehouse = "file:////path/to/warehouse"
```

## Examples

Here are some examples of what you can do, with an emphasis on the
inspect command which was the motivation behind this tool.

### Listing Objects

This command lists namespaces, tables and views; the optional pattern is GLOB-like.

```sh
# Usage
iceman list [PATTERN]
```

```sh
# Example
iceman list 'test.*'
type   name
table  test.t1
table  test.t3
```

### Inspecting Tables

This command can inspect tables and its metadata such as snapshots, manifests, and more.

```sh
# Usage
iceman inspect [OPTIONS] <IDENTIFIER> [TABLE]
```

These are the available metadata tables.

| Table                  | Scope                        | Usage                                 |
|------------------------|------------------------------|---------------------------------------|
| `manifests`            | current snapshot             | Current manifest state                |
| `files`                | current snapshot             | Active data & delete file stats       |
| `partitions`           | current snapshot             | Partition-level stats                 |
| `refs`                 | current                      | Branches and tags                     |
| `history`              | all lineage                  | Snapshot ancestry, rollback detection |
| `snapshots`            | all snapshots                | Snapshot inspection, time travel      |
| `all_data_files`       | all snapshots                | Cross-snapshot data file tracking     |
| `all_delete_files`     | all snapshots                | Cross-snapshot delete file tracking   |
| `all_entries`          | all snapshots                | All file ops (data + deletes)         |
| `all_manifests`        | all snapshots                | Cross-snapshot manifest tracking      |
| `entries`              | all snapshots                | Full audit trail (all file ops)       |
| `metadata_log_entries` | all metadata files           | Metadata file evolution               |

> Let namespace=test and table=events.

```sh
# List all snapshots for the 'events' table
iceman inspect test.events snapshots --limit 5

# List all data and delete files for the current snapshot of 'events'
iceman inspect test.events files --limit 5

# Inspect partition statistics like file counts and total bytes
iceman inspect test.events partitions
```

Iceman does not show all columns by default; use `-v` to show all columns.

```sh
iceman inspect test.events files -v
```

### Piping Output

Iceman output is TSV by default, so it works well with unix pipes. The purpose
was to have table-like output which works well with unix pipes and no additional
tokens. You can use the `--output json` flag to write output as JSON for 'jq'.

```sh

# reading wide tables with a pager with left-right scrolling
iceman inspect test.events files -v | less -S

# total bytes across all data files
iceman inspect test.events files \
  | awk -F'\t' 'NR>1 {sum+=$3} END {print sum}'

# extract just the file paths
iceman inspect test.events files \
  | cut -f4 | tail -n +2

# re-align the TSV for visual reading in another tool
iceman inspect test.events files \
  | column -t -s$'\t' | less -S

# count rows
iceman inspect test.events files --output json \
  | jq -s 'length'

# stream one field per line — no .[] needed
iceman inspect test.events files --output json \
  | jq -r '.file_path'

# filter and reshape
iceman inspect test.events snapshots --output json \
  | jq -r 'select(.operation == "append") | "\(.snapshot_id)\t\(.timestamp_ms)"'
```

### Using SQL

The `iceman inspect` command accepts a `-q <SQL>` argument. When given,
iceman will execute the SQL with `duckdb` against views named after
the metadata tables. This requires `duckdb` on PATH (`brew install duckdb`).

> DuckDB is not bundled because it makes 'iceman' 80MB vs. 8MB without.

```sh
# bytes added per snapshot
iceman inspect test.events -q "
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
iceman inspect test.events -q "
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

## License

Apache-2.0
