# S6 name and non-file custody failure ledger

> Append-only diagnostics, 2026-10-06. No numeric latency/cache or release claim.

- `build-nonfile-1.log` built with an unused orphan-module import after expanding
  the domain to non-files. Removed that import. The temporary patch script later
  failed matching a formatted readlink body; applied the remaining source edit
  directly, without rerunning its already-applied mutations.
- `build-nonfile-proof.log`/`nonfile-first.log`: proof passed with an unused test
  import. Removed the unused import before all-target Clippy.
- `build-name-bits-1.log`: used an unavailable `PathName::new_bytes`; the owning
  API is `PathName::from_bytes(&[u8])`. Fixed the checked boundary conversion.
- `overlay-first.log`: a newly added `NameLayers.active_inherited` fixture field
  guessed false over a captured binding. Its correct relative-lower fact is true.
  Updated the exact expectation. The known-install fixture now explicitly retains
  that true bit over the equivalent new base, even when a later caller supplies
  false; no per-name install rewrite or unsafe whiteout deletion occurs.
- `workspace-first.log`: the earlier profile expected a second fact round for
  directory rename. A stored active inheritance bit now decides it with one round
  and zero base demand. Updated the expectation from the source/retained counts.
  Unrelated-scale point-work equality still passes; this is not a speedup sample.
- `clippy-first.log`: the extracted canonical-root test helper returned a named
  binding directly; removed the unnecessary binding rather than suppress lint.

All original logs remain. No test reached 120 seconds. Build-first and explicit
<=120s test ceilings apply. Mac daemon/SDK consumer verification overlapped the
independent Linux target compilation; this is declared interference and no wall
or latency metric is qualified from that run. These unit/counter checks are not
performance samples. Third-party source/pins and unrelated containers/files are
unchanged.
