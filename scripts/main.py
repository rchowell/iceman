from __future__ import annotations

import argparse
import re
import string
from dataclasses import dataclass, field
from datetime import date, datetime
from pathlib import Path

import numpy as np
import pyarrow as pa
import pyarrow.compute as pc
import yaml
from pyiceberg.catalog import load_catalog
from pyiceberg.partitioning import PartitionField, PartitionSpec
from pyiceberg.schema import Schema
from pyiceberg.table.sorting import NullOrder, SortDirection, SortField, SortOrder
from pyiceberg.transforms import (
    BucketTransform,
    DayTransform,
    HourTransform,
    IdentityTransform,
    MonthTransform,
    TruncateTransform,
    YearTransform,
)
from pyiceberg.types import (
    BinaryType,
    BooleanType,
    DateType,
    DecimalType,
    DoubleType,
    FloatType,
    IntegerType,
    LongType,
    NestedField,
    StringType,
    TimestampType,
    TimestamptzType,
)

ONE_GB = 1_000_000_000

# ---------------------------------------------------------------------------
# Spec dataclasses
# ---------------------------------------------------------------------------

@dataclass
class ColumnDef:
    name: str
    type: str
    nullable: bool = False
    distribution: str = "uniform"
    range: list | None = None
    values: list[str] | None = None
    length: list[int] | None = None
    exponent: float = 1.5
    mean: float = 0.0
    stddev: float = 1.0
    true_pct: float = 0.5
    null_pct: float = 0.0


@dataclass
class PartitionDef:
    source: str
    transform: str
    args: int | None = None


@dataclass
class SortDef:
    source: str
    direction: str = "asc"
    null_order: str = "last"


@dataclass
class FileBucketsDef:
    column: str
    num_buckets: int
    bucket_width: float
    rows_per_file: int | None = None


@dataclass
class TableSpec:
    table: str
    scale: float
    columns: list[ColumnDef] = field(default_factory=list)
    partitions: list[PartitionDef] = field(default_factory=list)
    sort_order: list[SortDef] = field(default_factory=list)
    sort_data_by: list[str] = field(default_factory=list)
    write_chunk_size: int | None = None
    snapshots: int | None = None
    properties: dict[str, str] = field(default_factory=dict)
    delete_filter: str | None = None
    delete_pct: float = 0.0
    file_buckets: FileBucketsDef | None = None
    compression_ratio: float = 1.0

    @property
    def target_bytes(self) -> int:
        return int(self.scale * self.compression_ratio * ONE_GB)


# ---------------------------------------------------------------------------
# Spec loading
# ---------------------------------------------------------------------------

TYPE_DEFAULTS: dict[str, dict] = {
    "boolean": {"distribution": "uniform", "true_pct": 0.5},
    "int": {"distribution": "uniform", "range": [0, 2**31 - 1]},
    "long": {"distribution": "uniform", "range": [0, 2**63 - 1]},
    "float": {"distribution": "uniform", "range": [0.0, 1.0]},
    "double": {"distribution": "uniform", "range": [0.0, 1.0]},
    "string": {"distribution": "uniform", "length": [5, 20]},
    "date": {"distribution": "uniform", "range": ["2020-01-01", "2025-12-31"]},
    "timestamp": {
        "distribution": "uniform",
        "range": ["2020-01-01T00:00:00", "2025-12-31T23:59:59"],
    },
    "timestamptz": {
        "distribution": "uniform",
        "range": ["2020-01-01T00:00:00", "2025-12-31T23:59:59"],
    },
    "binary": {"distribution": "uniform", "length": [8, 64]},
}


def load_spec(path: str | Path, scale_override: float | None = None) -> TableSpec:
    raw = yaml.safe_load(Path(path).read_text())

    scale = scale_override if scale_override is not None else raw.get("scale", 1)

    columns = []
    for name, defn in raw["schema"].items():
        base_type = defn["type"].split("(")[0]
        defaults = TYPE_DEFAULTS.get(base_type, {})
        columns.append(ColumnDef(
            name=name,
            type=defn["type"],
            nullable=defn.get("nullable", True),
            distribution=defn.get("distribution", defaults.get("distribution", "uniform")),
            range=defn.get("range", defaults.get("range")),
            values=defn.get("values"),
            length=defn.get("length", defaults.get("length")),
            exponent=defn.get("exponent", 1.5),
            mean=defn.get("mean", 0.0),
            stddev=defn.get("stddev", 1.0),
            true_pct=defn.get("true_pct", 0.5),
            null_pct=defn.get("null_pct", 0.0),
        ))

    partitions = []
    for name, defn in raw.get("partitions", {}).items():
        partitions.append(PartitionDef(
            source=name,
            transform=defn["transform"],
            args=defn.get("args"),
        ))

    sort_order = []
    for name, defn in raw.get("order_by", {}).items():
        sort_order.append(SortDef(
            source=name,
            direction=defn.get("direction", "asc"),
            null_order=defn.get("null_order", "last"),
        ))

    properties = {}
    for k, v in raw.get("properties", {}).items():
        properties[str(k)] = str(v)

    fb_raw = raw.get("file_buckets")
    file_buckets = None
    if fb_raw:
        file_buckets = FileBucketsDef(
            column=fb_raw["column"],
            num_buckets=int(fb_raw["num_buckets"]),
            bucket_width=float(fb_raw["bucket_width"]),
            rows_per_file=fb_raw.get("rows_per_file"),
        )

    return TableSpec(
        table=raw["table"],
        scale=scale,
        columns=columns,
        partitions=partitions,
        sort_order=sort_order,
        sort_data_by=raw.get("sort_data_by", []),
        write_chunk_size=raw.get("write_chunk_size"),
        snapshots=raw.get("snapshots"),
        properties=properties,
        delete_filter=raw.get("delete_filter"),
        delete_pct=raw.get("delete_pct", 0.0),
        file_buckets=file_buckets,
        compression_ratio=raw.get("compression_ratio", 1.0),
    )


# ---------------------------------------------------------------------------
# PyIceberg builders
# ---------------------------------------------------------------------------

ICEBERG_TYPES = {
    "boolean": BooleanType,
    "int": IntegerType,
    "long": LongType,
    "float": FloatType,
    "double": DoubleType,
    "string": StringType,
    "date": DateType,
    "timestamp": TimestampType,
    "timestamptz": TimestamptzType,
    "binary": BinaryType,
}

DECIMAL_RE = re.compile(r"^decimal\((\d+),\s*(\d+)\)$")


def _iceberg_type(type_str: str):
    m = DECIMAL_RE.match(type_str)
    if m:
        return DecimalType(int(m.group(1)), int(m.group(2)))
    cls = ICEBERG_TYPES.get(type_str)
    if cls is None:
        raise ValueError(f"Unknown type: {type_str}")
    return cls()


def build_schema(spec: TableSpec) -> Schema:
    fields = []
    for i, col in enumerate(spec.columns, start=1):
        fields.append(NestedField(
            field_id=i,
            name=col.name,
            field_type=_iceberg_type(col.type),
            required=not col.nullable,
        ))
    return Schema(*fields)


TRANSFORMS = {
    "identity": lambda _: IdentityTransform(),
    "day": lambda _: DayTransform(),
    "month": lambda _: MonthTransform(),
    "year": lambda _: YearTransform(),
    "hour": lambda _: HourTransform(),
    "bucket": lambda args: BucketTransform(args),
    "truncate": lambda args: TruncateTransform(args),
}


def build_partition_spec(schema: Schema, spec: TableSpec) -> PartitionSpec:
    if not spec.partitions:
        return PartitionSpec()

    fields_by_name = {f.name: f for f in schema.fields}
    pfields = []
    for i, pdef in enumerate(spec.partitions):
        src = fields_by_name[pdef.source]
        transform_fn = TRANSFORMS.get(pdef.transform)
        if transform_fn is None:
            raise ValueError(f"Unknown transform: {pdef.transform}")
        pfields.append(PartitionField(
            source_id=src.field_id,
            field_id=1000 + i,
            transform=transform_fn(pdef.args),
            name=f"{pdef.source}_{pdef.transform}",
        ))
    return PartitionSpec(*pfields)


def build_sort_order(schema: Schema, spec: TableSpec) -> SortOrder:
    if not spec.sort_order:
        return SortOrder()

    fields_by_name = {f.name: f for f in schema.fields}
    sfields = []
    for sdef in spec.sort_order:
        src = fields_by_name[sdef.source]
        direction = SortDirection.ASC if sdef.direction == "asc" else SortDirection.DESC
        null_order = NullOrder.NULLS_LAST if sdef.null_order == "last" else NullOrder.NULLS_FIRST
        sfields.append(SortField(
            source_id=src.field_id,
            transform=IdentityTransform(),
            direction=direction,
            null_order=null_order,
        ))
    return SortOrder(*sfields)


# ---------------------------------------------------------------------------
# Data generation
# ---------------------------------------------------------------------------

class DataGenerator:
    def __init__(self, rng: np.random.Generator):
        self.rng = rng
        self._sequence_offset = 0

    def generate_column(self, col: ColumnDef, size: int) -> pa.Array:
        base_type = col.type.split("(")[0]

        if col.distribution == "sequence":
            data = self._gen_sequence(col, size)
        else:
            method = getattr(self, f"_gen_{base_type}", None)
            if method is None:
                raise ValueError(f"No generator for type: {col.type}")
            data = method(col, size)

        if col.nullable and col.null_pct > 0:
            mask = self.rng.random(size) < col.null_pct
            if isinstance(data, pa.Array):
                data = pa.array(data.to_pylist(), mask=mask)
            else:
                data = pa.array(data, mask=mask)

        return data if isinstance(data, pa.Array) else pa.array(data)

    def _gen_sequence(self, col: ColumnDef, size: int) -> np.ndarray:
        start = self._sequence_offset
        self._sequence_offset += size
        base_type = col.type.split("(")[0]
        dtype = np.int32 if base_type == "int" else np.int64
        return np.arange(start, start + size, dtype=dtype)

    def _distribution_ints(self, col: ColumnDef, size: int, lo: int, hi: int) -> np.ndarray:
        if col.distribution == "uniform":
            return self.rng.integers(lo, hi + 1, size=size)
        elif col.distribution == "normal":
            mean = col.mean if col.mean != 0.0 else (lo + hi) / 2
            std = col.stddev if col.stddev != 1.0 else (hi - lo) / 6
            vals = self.rng.normal(mean, std, size=size)
            return np.clip(vals, lo, hi).astype(np.int64)
        elif col.distribution == "zipf":
            exp = max(col.exponent, 1.01)
            raw = self.rng.zipf(exp, size=size)
            return lo + (raw - 1) % (hi - lo + 1)
        elif col.distribution == "skewed":
            vals = self.rng.beta(2, 5, size=size)
            return (lo + vals * (hi - lo)).astype(np.int64)
        raise ValueError(f"Unknown distribution: {col.distribution}")

    def _distribution_floats(self, col: ColumnDef, size: int, lo: float, hi: float) -> np.ndarray:
        if col.distribution == "uniform":
            return self.rng.uniform(lo, hi, size=size)
        elif col.distribution == "normal":
            mean = col.mean if col.mean != 0.0 else (lo + hi) / 2
            std = col.stddev if col.stddev != 1.0 else (hi - lo) / 6
            vals = self.rng.normal(mean, std, size=size)
            return np.clip(vals, lo, hi)
        elif col.distribution == "zipf":
            exp = max(col.exponent, 1.01)
            raw = self.rng.zipf(exp, size=size).astype(np.float64)
            return lo + (raw - 1) % (hi - lo)
        elif col.distribution == "skewed":
            vals = self.rng.beta(2, 5, size=size)
            return lo + vals * (hi - lo)
        raise ValueError(f"Unknown distribution: {col.distribution}")

    def _gen_boolean(self, col: ColumnDef, size: int) -> np.ndarray:
        return self.rng.random(size) < col.true_pct

    def _gen_int(self, col: ColumnDef, size: int) -> np.ndarray:
        lo, hi = col.range or [0, 2**31 - 1]
        return self._distribution_ints(col, size, int(lo), int(hi)).astype(np.int32)

    def _gen_long(self, col: ColumnDef, size: int) -> np.ndarray:
        lo, hi = col.range or [0, 2**63 - 1]
        return self._distribution_ints(col, size, int(lo), int(hi))

    def _gen_float(self, col: ColumnDef, size: int) -> np.ndarray:
        lo, hi = col.range or [0.0, 1.0]
        return self._distribution_floats(col, size, float(lo), float(hi)).astype(np.float32)

    def _gen_double(self, col: ColumnDef, size: int) -> np.ndarray:
        lo, hi = col.range or [0.0, 1.0]
        return self._distribution_floats(col, size, float(lo), float(hi))

    def _gen_decimal(self, col: ColumnDef, size: int) -> pa.Array:
        m = DECIMAL_RE.match(col.type)
        assert m is not None, f"Invalid decimal type: {col.type}"
        precision, scale = int(m.group(1)), int(m.group(2))
        lo, hi = col.range or [0.0, 10 ** (precision - scale)]
        vals = self._distribution_floats(col, size, float(lo), float(hi))
        quantized = np.round(vals, scale)
        return pa.array(quantized.tolist(), type=pa.decimal128(precision, scale))

    def _gen_string(self, col: ColumnDef, size: int) -> pa.Array:
        if col.values:
            indices = self._pick_indices(col, size, len(col.values))
            return pa.array([col.values[i] for i in indices], type=pa.string())
        lo, hi = col.length or [5, 20]
        chars = list(string.ascii_letters)
        max_len = int(hi)
        pool_size = min(size, 10000)
        pool = ["".join(self.rng.choice(chars, max_len)) for _ in range(pool_size)]
        indices = self.rng.integers(0, pool_size, size=size)
        if lo == hi:
            return pa.array([pool[indices[i]] for i in range(size)], type=pa.string())
        lengths = self.rng.integers(lo, hi + 1, size=size)
        return pa.array([pool[indices[i]][:lengths[i]] for i in range(size)], type=pa.string())

    def _gen_date(self, col: ColumnDef, size: int) -> pa.Array:
        lo_str, hi_str = col.range or ["2020-01-01", "2025-12-31"]
        lo_date = date.fromisoformat(str(lo_str))
        hi_date = date.fromisoformat(str(hi_str))
        lo_ord, hi_ord = lo_date.toordinal(), hi_date.toordinal()
        ordinals = self._distribution_ints(col, size, lo_ord, hi_ord)
        epoch_ord = date(1970, 1, 1).toordinal()
        days_since_epoch = (ordinals - epoch_ord).astype(np.int32)
        return pa.array(days_since_epoch, type=pa.date32())

    def _gen_timestamp(self, col: ColumnDef, size: int) -> pa.Array:
        lo_str, hi_str = col.range or ["2020-01-01T00:00:00", "2025-12-31T23:59:59"]
        lo_ts = int(datetime.fromisoformat(str(lo_str)).timestamp() * 1_000_000)
        hi_ts = int(datetime.fromisoformat(str(hi_str)).timestamp() * 1_000_000)
        micros = self._distribution_ints(col, size, lo_ts, hi_ts)
        tz = "UTC" if col.type == "timestamptz" else None
        return pa.array(micros, type=pa.timestamp("us", tz=tz))

    _gen_timestamptz = _gen_timestamp

    def _gen_binary(self, col: ColumnDef, size: int) -> pa.Array:
        lo, hi = col.length or [8, 64]
        lengths = self.rng.integers(lo, hi + 1, size=size)
        return pa.array([self.rng.bytes(int(length)) for length in lengths], type=pa.binary())

    def _pick_indices(self, col: ColumnDef, size: int, n: int) -> np.ndarray:
        if col.distribution == "uniform":
            return self.rng.integers(0, n, size=size)
        elif col.distribution == "round_robin":
            start = self._sequence_offset
            return np.arange(start, start + size) % n
        elif col.distribution == "zipf":
            exp = max(col.exponent, 1.01)
            raw = self.rng.zipf(exp, size=size)
            return (raw - 1) % n
        elif col.distribution == "normal":
            vals = self.rng.normal(n / 2, n / 6, size=size)
            return np.clip(vals, 0, n - 1).astype(np.int64)
        elif col.distribution == "skewed":
            vals = self.rng.beta(2, 5, size=size)
            return (vals * n).astype(np.int64) % n
        raise ValueError(f"Unknown distribution: {col.distribution}")


ARROW_TYPES = {
    "boolean": pa.bool_(),
    "int": pa.int32(),
    "long": pa.int64(),
    "float": pa.float32(),
    "double": pa.float64(),
    "string": pa.string(),
    "date": pa.date32(),
    "timestamp": pa.timestamp("us"),
    "timestamptz": pa.timestamp("us", tz="UTC"),
    "binary": pa.binary(),
}


def _arrow_type(col: ColumnDef) -> pa.DataType:
    base_type = col.type.split("(")[0]
    if base_type == "decimal":
        m = DECIMAL_RE.match(col.type)
        assert m is not None
        return pa.decimal128(int(m.group(1)), int(m.group(2)))
    at = ARROW_TYPES.get(base_type)
    if at is None:
        raise ValueError(f"No arrow type for: {col.type}")
    return at


def generate_batch(spec: TableSpec, gen: DataGenerator, size: int) -> pa.Table:
    arrays = {}
    fields = []
    for col in spec.columns:
        arrays[col.name] = gen.generate_column(col, size)
        fields.append(pa.field(col.name, _arrow_type(col), nullable=col.nullable))
    return pa.table(arrays, schema=pa.schema(fields))


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def _fmt_bytes(n: int) -> str:
    if n >= ONE_GB:
        return f"{n / ONE_GB:.2f} GB"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f} MB"
    return f"{n / 1_000:.1f} KB"


def create_and_populate(spec: TableSpec, catalog_name: str, batch_size: int, seed: int, dry_run: bool):
    schema = build_schema(spec)
    partition_spec = build_partition_spec(schema, spec)
    sort_order = build_sort_order(schema, spec)
    target = spec.target_bytes

    print(f"Table:  {spec.table}")
    print(f"Scale:  {spec.scale} (target {_fmt_bytes(target)})")
    print("Schema:")
    for col in spec.columns:
        nullable = " (nullable)" if col.nullable else ""
        print(f"  {col.name}: {col.type}{nullable} [{col.distribution}]")
    if spec.partitions:
        print("Partitions:")
        for p in spec.partitions:
            suffix = f"({p.args})" if p.args else ""
            print(f"  {p.source}: {p.transform}{suffix}")
    if spec.sort_order:
        print("Sort order:")
        for s in spec.sort_order:
            print(f"  {s.source}: {s.direction} nulls {s.null_order}")
    if spec.sort_data_by:
        print(f"Sort data by: {spec.sort_data_by}")
    if spec.write_chunk_size:
        print(f"Write chunk size: {spec.write_chunk_size:,}")
    if spec.file_buckets:
        fb = spec.file_buckets
        print(f"File buckets: {fb.num_buckets} × width {fb.bucket_width} on '{fb.column}'")
    if spec.snapshots:
        print(f"Snapshots: {spec.snapshots}")
    if spec.properties:
        print(f"Properties: {spec.properties}")
    if spec.delete_filter:
        print(f"Delete filter: {spec.delete_filter}")
    if spec.delete_pct > 0:
        print(f"Delete pct: {spec.delete_pct:.0%}")

    if dry_run:
        rng = np.random.default_rng(seed)
        gen = DataGenerator(rng)
        sample = generate_batch(spec, gen, min(1000, batch_size))
        bytes_per_row = sample.nbytes / sample.num_rows
        est_rows = int(target / bytes_per_row)
        print(f"\n[dry-run] ~{bytes_per_row:.0f} bytes/row, ~{est_rows:,} rows to reach {_fmt_bytes(target)}")
        return

    catalog = load_catalog(catalog_name)

    namespace = tuple(spec.table.split(".")[:-1])
    if namespace:
        try:
            catalog.create_namespace(namespace)
        except Exception:
            pass

    try:
        catalog.drop_table(spec.table)
        print(f"Dropped existing table {spec.table}")
    except Exception:
        pass

    table = catalog.create_table(
        spec.table,
        schema=schema,
        partition_spec=partition_spec,
        sort_order=sort_order,
        properties=spec.properties,
    )

    rng = np.random.default_rng(seed)
    gen = DataGenerator(rng)
    bytes_written = 0
    rows_written = 0

    if spec.sort_data_by:
        batches = []
        while bytes_written < target:
            batch = generate_batch(spec, gen, batch_size)
            bytes_written += batch.nbytes
            batches.append(batch)
        full_table = pa.concat_tables(batches)

        sort_keys = [(col, "ascending") for col in spec.sort_data_by]
        sorted_indices = pc.sort_indices(full_table, sort_keys=sort_keys)
        full_table = full_table.take(sorted_indices)

        total_rows = full_table.num_rows
        if spec.snapshots:
            snapshot_size = total_rows // spec.snapshots
            for snap_i in range(spec.snapshots):
                start = snap_i * snapshot_size
                end = total_rows if snap_i == spec.snapshots - 1 else start + snapshot_size
                table.append(full_table.slice(start, end - start))
                rows_written = end
                print(f"  snapshot {snap_i + 1}/{spec.snapshots}: wrote {end - start:,} rows ({rows_written:,} total)")
        else:
            chunk = spec.write_chunk_size or batch_size
            while rows_written < total_rows:
                end = min(rows_written + chunk, total_rows)
                batch = full_table.slice(rows_written, end - rows_written)
                table.append(batch)
                rows_written = end
                print(f"  wrote {rows_written:,} / {total_rows:,} rows ({_fmt_bytes(bytes_written)})")
    elif spec.file_buckets:
        fb = spec.file_buckets
        target_col = next(c for c in spec.columns if c.name == fb.column)

        if fb.rows_per_file:
            rows_per_file = fb.rows_per_file
        else:
            sample = generate_batch(spec, gen, 1000)
            bytes_per_row = sample.nbytes / sample.num_rows
            rows_per_file = max(1, int(target / fb.num_buckets / bytes_per_row))

        print(f"  rows per file: {rows_per_file:,}")

        for i in range(fb.num_buckets):
            lo = fb.bucket_width * i
            hi = fb.bucket_width * (i + 1)
            old_range = target_col.range
            target_col.range = [lo, hi]
            batch = generate_batch(spec, gen, rows_per_file)
            target_col.range = old_range
            table.append(batch)
            bytes_written += batch.nbytes
            rows_written += batch.num_rows
            if (i + 1) % 100 == 0 or i == fb.num_buckets - 1:
                print(f"  bucket {i + 1}/{fb.num_buckets}: value in [{lo}, {hi}) — {rows_written:,} rows total")
    else:
        if spec.snapshots:
            batches = []
            while bytes_written < target:
                batch = generate_batch(spec, gen, batch_size)
                bytes_written += batch.nbytes
                batches.append(batch)
            full_table = pa.concat_tables(batches)
            total_rows = full_table.num_rows
            snapshot_size = total_rows // spec.snapshots
            for snap_i in range(spec.snapshots):
                start = snap_i * snapshot_size
                end = total_rows if snap_i == spec.snapshots - 1 else start + snapshot_size
                table.append(full_table.slice(start, end - start))
                rows_written = end
                print(f"  snapshot {snap_i + 1}/{spec.snapshots}: wrote {end - start:,} rows ({rows_written:,} total)")
        else:
            while bytes_written < target:
                batch = generate_batch(spec, gen, batch_size)
                batch_bytes = batch.nbytes
                table.append(batch)
                bytes_written += batch_bytes
                rows_written += batch.num_rows
                print(f"  wrote {rows_written:,} rows ({_fmt_bytes(bytes_written)} / {_fmt_bytes(target)})")

    if spec.delete_filter:
        print(f"Applying delete filter: {spec.delete_filter}")
        table.delete(delete_filter=spec.delete_filter)

    if spec.delete_pct > 0 and rows_written > 0:
        threshold = int(rows_written * spec.delete_pct)
        delete_expr = f"id < {threshold}"
        print(f"Deleting {spec.delete_pct:.0%} of rows: {delete_expr}")
        table.delete(delete_filter=delete_expr)

    print(f"Done. {spec.table}: {rows_written:,} rows, {_fmt_bytes(bytes_written)}.")


def main():
    parser = argparse.ArgumentParser(description="Generate Iceberg test tables from YAML specs")
    parser.add_argument("spec", help="Path to YAML spec file")
    parser.add_argument("--catalog", default="default", help="Catalog name (default: default)")
    parser.add_argument("--namespace", default=None, help="Override namespace (e.g. sf_1)")
    parser.add_argument("--scale", type=float, default=None, help="Override scale (1 = 1 GB)")
    parser.add_argument("--batch-size", type=int, default=500_000, help="Rows per write batch")
    parser.add_argument("--seed", type=int, default=42, help="RNG seed for reproducibility")
    parser.add_argument("--dry-run", action="store_true", help="Validate and print spec without writing")
    args = parser.parse_args()

    spec = load_spec(args.spec, args.scale)

    if args.namespace is not None:
        table_name = spec.table.split(".")[-1]
        spec.table = f"{args.namespace}.{table_name}"

    create_and_populate(spec, args.catalog, args.batch_size, args.seed, args.dry_run)


if __name__ == "__main__":
    main()
