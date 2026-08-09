# LayerFS benchmark evidence

The `c3-*` files are the preserved preselection evidence produced while NF, OF, and
OS were compared under the original NF-relative qualification contract. Their valid
terminal result is No Selection under those superseded gates. They are historical
audit artifacts; references to NF in those files do not indicate a live NF code path.

The `l1.5-final-*` files are the first post-retirement OF/OS-only run. The
`l1.5-closure-*` files are the custody-grade rerun from the final release executable
after source and runner reconciliation. The latter are the controlling performance
evidence for the owner-approved L1.5 closure:

- `l1.5-closure-of-os-anchor.jsonl`: 64 MiB streamed scanner, one warmup and three
  measured samples per algorithm;
- `l1.5-closure-of-os-prng-8m.jsonl`: fifteen same-round complete FsCas-backed C3
  samples per algorithm on PRNG input;
- `l1.5-closure-of-os-repeated-8m.jsonl`: fifteen same-round complete FsCas-backed C3
  samples per algorithm on repeated input.

`l1.5-completion-custody.json` records the final source, registry, executable,
toolchain, platform, and controlling-evidence hashes. Historical manifests remain
unchanged because rewriting their hashes would destroy their value as records of the
earlier run.
