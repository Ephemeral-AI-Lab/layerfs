# Generated JSON

[Architecture](../../README.md) › [Control](../README.md) › [Support matrix](README.md) › Generated JSON

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

SUPPORT.json and the test that keeps it in step.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Support matrix](README.md) |
| Source | [`control/SUPPORT.json`](../../../../control/SUPPORT.json), [`control/crates/layerfs-platform/tests/support.rs`](../../../../control/crates/layerfs-platform/tests/support.rs), `support_json()` in [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs), [`layerfs-isolation/tests/support/support_json.rs`](../../../../control/crates/layerfs-isolation/tests/support/support_json.rs) |

## SUPPORT.json

```text
{
  "generated_from": "control/crates/layerfs-platform/src/support.rs",
  "hosts": [                         HostOs::NAMED order
    { "host": "macos",
      "platforms": [                 Platform::ALL order, 7 rows
        { "platform":    "native",
          "how":         "built_in",
          "backend":     "native" | null,
          "kernel":      "nfs"    | null,
          "sqlite":      "in_process" | "worker" | null,
          "postgresql":  { "store_calls": "in_process",
                           "sandbox": true|false,
                           "reason": "<text>" | null },
          "options":     [ { "name": "cpus", "kind": "int" }, ... ],
          "description": "<profile>; <backend>" },
        ... ] },
    { "host": "linux",   ... },
    { "host": "windows", ... } ] }
```

| Property | Value | Source |
| --- | --- | --- |
| Producer | `support_json()`, a hand-written renderer: fixed key order, fixed indentation, strings quoted by `layerfs_isolation::json::quoted` | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| Hosts | `macos`, `linux`, `windows` (`HostOs::NAMED`); `other` is never written | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| Rows | `platforms(host)` for each host | `support.rs` |
| Live fields | none: `ready`, `missing` and `version` are added only by `machine.platforms` | [`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs) |
| Option kinds | `int`, `size`, `text`, `bool`, `name` (`OptKind::as_str`) | [`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs) |

The field meanings are in [Table](table.md#fields).

## Regenerating it

```text
 edit support.rs or a platforms/<p>/profile.rs
        |
        v
 LAYERFS_BLESS_SUPPORT=1 cargo test ... --test support
        |   support_json_is_the_rendered_table writes
        |   support_json() to control/SUPPORT.json and returns
        v
 review `git diff control/SUPPORT.json`, commit both together
```

The exact command, from [`control/README.md`](../../../../control/README.md):

```
LAYERFS_BLESS_SUPPORT=1 cargo +1.85.1 test --locked -p layerfs-platform --test support
```

Only the value `1` blesses; any other value, or none, compares. The test
writes the path `CARGO_MANIFEST_DIR/../../SUPPORT.json`, which is
`control/SUPPORT.json`.

## The comparing test

| Test | What it compares | On a difference | Source |
| --- | --- | --- | --- |
| `support_json_is_the_rendered_table` | The committed file against `support_json()`, as whole strings | fails: "control/SUPPORT.json differs from support.rs; rerun this test with LAYERFS_BLESS_SUPPORT=1" | [`tests/support.rs`](../../../../control/crates/layerfs-platform/tests/support.rs) |
| `each_platform_row_in_support_json_is_its_table_row` | The committed file, parsed back row by row, against `platforms(host)`: every field and the host set | names the platform, host and field | [`tests/conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) |
| `layerfs-isolation` conformance | Each backend's capabilities against the kernel routes the file lists | names the backend | [`layerfs-isolation/tests/conformance.rs`](../../../../control/crates/layerfs-isolation/tests/conformance.rs) |

The two conformance kits read the committed file through
`support_json.rs`, a small reader built on `layerfs_isolation::json::Json`,
so they check what callers and these pages read, not only the Rust table.
The commands, not run while writing this page:

```text
cargo +1.85.1 test --locked -p layerfs-platform --test support
cargo +1.85.1 test --locked -p layerfs-platform --test conformance
cargo +1.85.1 test --locked -p layerfs-isolation --test conformance
```

All three are `#![cfg(unix)]` and need no Docker, VM, ssh host or root.

## Related

- [Table](table.md): the rows and fields this file renders.
- [Machines: Adding a platform](../machines/adding-a-platform.md): when a
  new platform's rows are blessed.
- [Control: Management](../management/README.md): `machine.platforms`, the
  live form of the same rows.

---

← [Support matrix: Table](table.md) · [Up: Support matrix](README.md) · [Control: Machines](../machines/README.md) →
