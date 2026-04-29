# iceman command reference

Compact reference for every `iceman` subcommand and flag. For SQL guidance over
metadata views, see `sql.md`. For catalog config, see `config.md`.

## Global flags

These apply to every subcommand:

| Flag                         | Purpose                                                  |
|------------------------------|----------------------------------------------------------|
| `-c, --config PATH`          | Path to config file. Env: `ICEMAN_CONFIG`.               |
| `--catalog NAME`             | Pick a named catalog from the config.                    |
| `-v, --verbose`              | Verbose output.                                          |
| `--output text\|json`        | Output format. Default `text`.                           |
| `--uri URI`                  | Override catalog URI (also used for type inference).     |
| `--credential CRED`          | Override catalog credential.                             |
| `--warehouse PATH`           | Override warehouse location.                             |

## `iceman list [PATTERN]`

Walks every namespace (recursively) and lists both namespaces and tables.

- `PATTERN` is an optional glob filter applied to the full dot-separated identifier
  (e.g. `analytics.*`, `*orders*`, `db.schema.fact_*`).
- Output columns: `type` (`namespace` | `table`), `name`.

```
iceman list
iceman list 'analytics.*'
iceman list --output json | jq '.[] | select(.type=="table") | .name'
```

## `iceman describe IDENT`

Auto-detects whether `IDENT` is a namespace or a table and prints its description.

| Flag                  | Purpose                                                       |
|-----------------------|---------------------------------------------------------------|
| `--entity any`        | Default. Try table first, fall back to namespace.             |
| `--entity namespace`  | Force namespace lookup.                                       |
| `--entity table`      | Force table lookup.                                           |

```
iceman describe analytics
iceman describe analytics.events --entity table
```

## `iceman inspect IDENT [METADATA_TABLE]`

Inspect a table's internals. Two modes:

**Typed mode** (positional metadata table):

| Value             | Returns                                                  |
|-------------------|----------------------------------------------------------|
| `snapshots`       | All snapshots.                                           |
| `history`         | Snapshot ancestry of the current ref.                    |
| `metadata-log`    | Metadata file evolution.                                 |
| `refs`            | Branches and tags.                                       |
| `manifests`       | Manifests in the current (or `--snapshot-id`) snapshot.  |
| `all-manifests`   | Manifests across all snapshots.                          |
| `entries`         | Manifest entries in the current snapshot.                |
| `all-entries`     | Manifest entries across all snapshots.                   |
| `files`           | Live data + delete files.                                |
| `data-files`      | Live data files only.                                    |
| `delete-files`    | Live delete files only.                                  |
| `all-data-files`  | Data files across all snapshots.                         |
| `all-delete-files`| Delete files across all snapshots.                       |
| `partitions`      | Per-partition record/file counts.                        |

**SQL mode** (`-q / --query`):

Runs DuckDB SQL across the metadata views. The view set is the same as the typed
mode names (snake_case: `snapshots`, `history`, `metadata_log_entries`, `refs`,
`manifests`, `all_manifests`, `entries`, `all_entries`, `files`, `data_files`,
`delete_files`, `all_data_files`, `all_delete_files`, `partitions`). Mutually
exclusive with the positional metadata table.

| Flag                   | Purpose                                                |
|------------------------|--------------------------------------------------------|
| `-q, --query SQL`      | DuckDB SQL over the metadata views.                    |
| `--snapshot-id N`      | Use snapshot `N` instead of current (typed mode only). |
| `--limit N`            | Cap displayed rows.                                    |

```
iceman inspect analytics.events snapshots --limit 5
iceman inspect analytics.events files --snapshot-id 5723145 --output json
iceman inspect analytics.events -q \
  "SELECT count(*) AS n_files, sum(file_size_in_bytes) AS bytes FROM files"
```

## Exit behavior

- Returns non-zero on any error (catalog load failure, table not found, SQL parse error).
- `iceman list` on an empty catalog returns 0 with no rows.
- `iceman inspect` on a table with no current snapshot fails for snapshot-scoped
  metadata tables (`files`, `manifests`, `entries`, `partitions`, etc.) but succeeds
  for `snapshots`, `history`, `metadata_log_entries`, and `refs`.

## `iceman skill install [LOCATION]`

Installs this skill (the one you are reading) onto disk so Claude Code can pick it up.

| Position / Flag     | Purpose                                                      |
|---------------------|--------------------------------------------------------------|
| `LOCATION` (pos.)   | Parent dir; `iceman/` is created inside. Default `./.claude/skills`. |
| `--user`            | Install into `~/.claude/skills/iceman/` instead.             |
| `--force`           | Overwrite an existing install.                               |

```
iceman skill install                  # ./.claude/skills/iceman/
iceman skill install --user           # ~/.claude/skills/iceman/
iceman skill install /tmp/skills      # /tmp/skills/iceman/
iceman skill install --user --force   # refresh after upgrading iceman
```
