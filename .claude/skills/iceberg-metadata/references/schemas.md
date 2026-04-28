# Iceberg Metadata Table Schemas

## Table of Contents
1. [history](#history)
2. [metadata_log_entries](#metadata_log_entries)
3. [snapshots](#snapshots)
4. [entries / all_entries](#entries--all-entries)
5. [files](#files)
6. [manifests / all_manifests](#manifests--all_manifests)
7. [partitions](#partitions)
8. [all_data_files / all_delete_files](#all_data_files--all_delete_files)
9. [refs](#refs)

---

## history

Commit ancestry of the current table branch. One row per snapshot in the lineage.

| Field | Type | Description |
|---|---|---|
| `made_current_at` | timestamp | When this snapshot became the current snapshot |
| `snapshot_id` | long | Unique snapshot identifier |
| `parent_id` | long (nullable) | Parent snapshot ID; `null` for the first snapshot |
| `is_current_ancestor` | boolean | `true` if this snapshot is an ancestor of the current state; `false` indicates a rolled-back snapshot |

**Rollback detection:** If a `parent_id` appears more than once in this table, a rollback occurred.

---

## metadata_log_entries

History of `metadata.json` file generations. One row per metadata file written.

| Field | Type | Description |
|---|---|---|
| `timestamp` | timestamp | When the metadata file was created |
| `file` | string | Full path to the `metadata.json` file |
| `latest_snapshot_id` | long (nullable) | Snapshot ID captured in that metadata file |
| `latest_schema_id` | int (nullable) | Schema ID at time of metadata creation |
| `latest_sequence_number` | long (nullable) | Sequence number at time of metadata creation |

---

## snapshots

All snapshots in the table's retention window. One row per snapshot.

| Field | Type | Description |
|---|---|---|
| `committed_at` | timestamp | Wall-clock time of commit |
| `snapshot_id` | long | Unique snapshot identifier |
| `parent_id` | long (nullable) | Parent snapshot ID |
| `operation` | string | One of: `append`, `replace`, `overwrite`, `delete` |
| `manifest_list` | string | Path to the manifest list (Avro) file for this snapshot |
| `summary` | map<string,string> | Engine-provided key/value metadata (e.g., `added-records`, `total-records`, `spark.app.id`) |

**Common `summary` keys** (populated by most engines):
- `added-data-files`, `deleted-data-files`, `added-records`, `deleted-records`
- `total-data-files`, `total-records`, `total-files-size`
- `changed-partition-count`

---

## entries / all_entries

Every file operation across manifests. `entries` = current snapshot only.
`all_entries` = all snapshots (may have duplicate file rows across snapshots).

| Field | Type | Description |
|---|---|---|
| `status` | int | `0`=existing, `1`=added, `2`=deleted |
| `snapshot_id` | long | Snapshot where this operation occurred |
| `sequence_number` | long | Global monotonic counter; use to order all operations |
| `file_sequence_number` | long | Sequence number when the file was first added |
| `data_file` | struct | See [data_file struct](#data_file-struct) below |

### data_file struct

| Sub-field | Type | Description |
|---|---|---|
| `content` | int | `0`=data, `1`=position deletes, `2`=equality deletes |
| `file_path` | string | Full storage URI (e.g., `s3://...`) |
| `file_format` | string | `PARQUET`, `ORC`, `AVRO` |
| `partition` | struct | Partition values for this file |
| `record_count` | long | Total rows in the file |
| `file_size_in_bytes` | long | File size |
| `column_sizes` | map<int,long> | Byte size per column ID |
| `value_counts` | map<int,long> | Total value count per column ID |
| `null_value_counts` | map<int,long> | Null count per column ID |
| `nan_value_counts` | map<int,long> | NaN count per column ID (numeric columns only) |
| `lower_bounds` | map<int,binary> | Min value per column ID (engine decodes) |
| `upper_bounds` | map<int,binary> | Max value per column ID (engine decodes) |
| `key_metadata` | binary (nullable) | Implementation-specific encryption or key metadata |
| `split_offsets` | list<long> (nullable) | Row group / split boundary offsets |
| `equality_ids` | list<int> (nullable) | Column IDs used for equality delete matching |
| `sort_order_id` | int (nullable) | Sort order applied when writing |

---

## files

Active data and delete files in the **current snapshot**. Exposes `data_file` struct fields
as top-level columns for convenience. Schema mirrors the `data_file` struct above, promoted
to top-level, plus:

| Field | Type | Description |
|---|---|---|
| `content` | int | `0`=data, `1`=position deletes, `2`=equality deletes |
| `file_path` | string | Full URI |
| `file_format` | string | |
| `spec_id` | int | Partition spec ID used when writing this file |
| `partition` | struct | Partition values |
| `record_count` | long | |
| `file_size_in_bytes` | long | |
| `column_sizes` | map<int,long> | |
| `value_counts` | map<int,long> | |
| `null_value_counts` | map<int,long> | |
| `nan_value_counts` | map<int,long> | |
| `lower_bounds` | map<int,binary> | |
| `upper_bounds` | map<int,binary> | |
| `key_metadata` | binary (nullable) | |
| `split_offsets` | list<long> (nullable) | |
| `equality_ids` | list<int> (nullable) | |
| `sort_order_id` | int (nullable) | |
| `readable_metrics` | struct (nullable) | Human-readable decoded column stats (engine-dependent) |

---

## manifests / all_manifests

`manifests` = manifests in the current snapshot.
`all_manifests` = every manifest across all snapshots; one row per (manifest, snapshot) pair.

| Field | Type | Description |
|---|---|---|
| `content` | int | `0`=tracks data files, `1`=tracks delete files |
| `path` | string | Full path to the `.avro` manifest file |
| `length` | long | File size in bytes |
| `partition_spec_id` | int | Partition spec ID used when writing the manifest |
| `added_snapshot_id` | long | Snapshot that first added this manifest |
| `added_data_files_count` | int | Data files added in `added_snapshot_id` |
| `existing_data_files_count` | int | Data files carried over from prior snapshots |
| `deleted_data_files_count` | int | Data files marked deleted |
| `added_delete_files_count` | int | Delete files added |
| `existing_delete_files_count` | int | Delete files carried over |
| `deleted_delete_files_count` | int | Delete files marked deleted |
| `partition_summaries` | list<struct> | Per-partition: `{contains_null, contains_nan, lower_bound, upper_bound}` |
| `reference_snapshot_id` | long | Snapshot this row was valid for (`all_manifests` only) |

---

## partitions

Partition-level aggregate stats for the **current snapshot**.

| Field | Type | Description |
|---|---|---|
| `spec_id` | int | Partition spec ID |
| `partition` | struct | Partition field values |
| `record_count` | long | Total records across all files in this partition |
| `file_count` | int | Number of data files |
| `total_data_file_size_in_bytes` | long | Total size of data files |
| `position_delete_record_count` | long | Total positional delete records |
| `position_delete_file_count` | int | Number of positional delete files |
| `equality_delete_record_count` | long | Total equality delete records |
| `equality_delete_file_count` | int | Number of equality delete files |
| `last_updated_at` | timestamp (nullable) | Timestamp of last modification to partition |
| `last_updated_snapshot_id` | long (nullable) | Snapshot that last modified partition |

---

## all_data_files / all_delete_files

Cross-snapshot versions of `files` scoped to data files and delete files respectively.
May return the same file multiple times (once per snapshot it was valid in).

Schema is identical to `files` (see above) with the addition of:

| Field | Type | Description |
|---|---|---|
| `reference_snapshot_id` | long | Snapshot this row was valid for |

---

## refs

All named references (branches and tags).

| Field | Type | Description |
|---|---|---|
| `name` | string | Reference name (e.g., `main`, `etl-branch`, `v1.0`) |
| `type` | string | `BRANCH` (mutable) or `TAG` (immutable) |
| `snapshot_id` | long | Snapshot this reference currently points to |
| `max_reference_age_in_ms` | long (nullable) | Max age of the reference before it becomes a cleanup candidate |
| `min_snapshots_to_keep` | int (nullable) | Minimum retained snapshots for this reference |
| `max_snapshot_age_in_ms` | long (nullable) | Max age of any retained snapshot on this reference |

**Retention logic:** A snapshot is retained if it is newer than `max_snapshot_age_in_ms`
OR if the total retained snapshot count would drop below `min_snapshots_to_keep`.
`null` means no constraint is set.