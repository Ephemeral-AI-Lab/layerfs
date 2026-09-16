# Stages 3–4 smoke evidence

> **Status:** exploratory single-sample wiring evidence for #168/#169; not a
> benchmark campaign and not a release qualification.

One sample per case, fresh output directory per run, produced by
`core/crates/layerfs-storage/examples/measure_edits.rs`. Every run used the same
build (working tree at the commit that adds this directory); no cache contract is
declared beyond the fact that the fixture is built by the same process
immediately before the timed scope, and no warm-cache credit is claimed.

| Directory | Command (from the repository root) |
| --- | --- |
| `c1/` | `cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode c1 --case small --threshold-bytes 131072 --output <dir>` |
| `c2/` | `... --mode c2 --case small --threshold-bytes 131072 --output <dir>` |
| `pipeline/` | `... --mode pipeline --case chunked --threshold-bytes 131072 --output <dir>` |
| `grow/` | `... --mode pipeline --case small-to-large --threshold-bytes 1048576 --output <dir>` |
| `shrink/` | `... --mode pipeline --case large-to-small --threshold-bytes 1048576 --output <dir>` |
| `shrink2/` | repeat of `shrink` after the harness change that made base preparation untimed |
| `batch/` | `... --mode pipeline --case batch --threshold-bytes 262144 --output <dir>` |

Only the JSON timing reports are retained; the `store.sqlite` files were removed
from this evidence directory (the `shrink` store was about 3.5 MB, the `grow`
store about 2.1 MB). The reports are the raw artifacts; the numbers below are
copied from the run output.

## Recorded results

| Case | Result root | Logical length | Observed edit scope | Observed verification |
| --- | --- | ---: | ---: | ---: |
| c1 small | `8ddfe36cd5449f2b720590cb05da552290acaa5546ed065881c832703c650798` | 65 536 | `file.edit` 2.551 ms (observed once) | n/a (DB-free) |
| c2 small | n/a (supplied canonical objects) | 65 536 | `storage.save` 42.079 ms (observed once) | 65 559 bytes verified |
| pipeline chunked | recorded in `pipeline/` | 262 144 | recorded in `pipeline/pipeline-edit-save.json` | verified byte-for-byte against the model |
| pipeline small-to-large | `4fb197e6bda95fb1f877f41da2750ae2d80c3f6352a618d2a1bd0d318418b8e3` | 2 097 151 | `edit.save` 158.824 ms (observed once) | verified byte-for-byte against the model |
| pipeline large-to-small | `5bdb5824866a79c78b79e140230c30eb16a61736a8d4c0df3e066406685c54e0` | 1 048 576 | `edit.save` 5.970 ms (observed once) | `verify.readback` 54.928 ms, verified |
| pipeline batch | recorded in `batch/` | 390 916 | recorded in `batch/pipeline-edit-save.json` | verified byte-for-byte against the model |

The `large-to-small` run also shows the decomposition the contract asks for:

```text
edit.save              5.970ms
  storage.begin        2.751ms
  file.edit            2.638ms
    edit.base          1.237ms
      edit.base_read   1.233ms
    edit.compare       1.167us
    edit.stream        1.307ms
    edit.finish       81.334us
  storage.finish     468.916us
verify.readback       54.928ms
  content.read        54.925ms
    content.acquire    1.044ms
    content.traverse  53.877ms
```

Base preparation is untimed and separate (the base is acknowledged before the
timed scope) and the readback is a second, separately recorded scope. Overlapping
spans are inclusive and are never subtracted from one another.

## Not claimed

* No v0.1.6 comparison is made here: no matched reference arm, cache contract or
  registered campaign was collected for these cases.
* One sample per case: no statistical claim, no confidence interval.
* The numbers are not a performance gate and were not used to tune anything.
