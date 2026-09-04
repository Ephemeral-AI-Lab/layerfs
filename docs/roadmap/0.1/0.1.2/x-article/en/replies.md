# LayerFS v0.1.2 story-led X replies

Post these three replies under the English X Article announcement. Copy the
text inside each block and attach the listed images.

## Reply 1/3 — The puzzle

```text
1/3 — Imagine adding one page to the front of a giant book.

The slow solution rewrites every old page after it.

LayerFS keeps the old pages and changes a small table of contents:

new page | pointer to the same old book

That is the idea behind v0.1.2. A live Workspace keeps a draft list of old pieces and new pieces. Commit saves the final list into the existing extent tree.

The extent tree is older than v0.1.0. What v0.1.2 adds is the bridge from live edits to that saved, reference-based structure.

One recipe covers prepend, append, overwrite, insert, delete, grow, shrink, truncate, and zero-extension:

old prefix + replacement + old suffix

Change the pointers. Keep the unchanged bytes.
```

Attach:

- `./images/01-edit-locality.png`
- `./images/02-workspace-edit-pipeline.png`

## Reply 2/3 — The algorithm and proof

```text
2/3 — The Big-O story is simple.

Let N be the old file size and a be the new bytes.

Copy-based edit: Θ(N + a)
LayerFS draft edit: O(a + tree height + removed pieces)
Whole-file read or hash: still Θ(N)

LayerFS does not make every operation O(1). It removes the forced full-file rewrite from a known-range edit.

Then we tested whether the code behaved like the idea.

A 4 KiB prepend plus Commit took:

• 1 MiB file: 4.680 ms
• 10 MiB: 4.883 ms
• 100 MiB: 7.257 ms
• 500 MiB: 14.300 ms

And we did not stop at one lucky case: 56 edit cases, 560 timed runs, and 112 separate correctness proofs across 1/10/100/500 MiB files.
```

Attach:

- `./images/06-big-o-table.png`
- `./images/03-prepend-scaling.png`
- `./images/05-evidence-matrix.png`

## Reply 3/3 — The comparison and boundary

```text
3/3 — We also tested a pinned Cloudflare Computer real-FUSE path.

For three 4 KiB edits on a 100 MiB file:

• Overwrite: LayerFS 6.928 ms / Cloudflare path 225.8 ms — 33×
• Middle insert: 7.752 ms / 3,040.9 ms — 392×
• Prepend: 7.257 ms / 5,827.6 ms — 803×

Why? The measured Cloudflare path builds buffered file state and moves bytes for positional edits. LayerFS changes references and publishes new bytes plus changed tree nodes.

The APIs and timing boundaries differ, so this is not a universal “803× faster product” claim. It compares complete measured paths at pinned source. The Cloudflare campaign completed 168 timed runs and 168 separate byte-correctness checks.

LayerFS v0.1.2 remains a source-only Developer Preview, not a crash or power-loss durability promise.

https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.2
```

Attach:

- `./images/04-cloudflare-comparison.png`
