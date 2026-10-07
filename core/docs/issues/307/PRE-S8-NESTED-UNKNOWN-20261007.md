# Preserve nested Storage uncertainty through Commit

> **Status:** Implemented correction with Disposable host/Linux proof after
> `a5f8b7f27`; no performance run.

Source review during F5 found that StorageError::is_unknown_outcome recognizes
only its top-level UnknownOutcome, while its public CleanupFailed variant can
carry an original or cleanup unknown. Current Storage producers do not construct
that wrapper, but the public daemon Content producer can return it. Commit uses
this method to decide whether local capture may be resolved, so the helper must
preserve uncertainty at either nested position.

- Reuse the real installed-Store/overlay Commit fixture and original producer
  callback. Extend `daemon/tests/store_commit.rs` with nested original/cleanup
  unknowns and a definite control case; prove retained capture/no local resolution,
  preserved live bytes, no stage row and refusal of another Commit for unknowns.
- Run the focused regression first, retaining its expected failure. Change only
  `storage/src/store/error.rs` so CleanupFailed recursively retains either cause's
  uncertainty. No new provider hook, retry, read-based settlement or cleanup.
- Build first, run that body on host then the pinned Linux image, under100s stops.
  Prior F8/F5 behavior is otherwise unchanged; reuse its covering proofs. Finish
  scoped all-target Clippy/fmt/boundary/self-tests, docs and exact staged LOC.

## Evidence and original unknown custody

01 builds the public-producer regression.02 fails exactly at the first nested
unknown: Commit incorrectly reports local resolution. Its failed fixture remains
at `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-14579-nested-unknown`.
After the shared helper correction,03/04 build and pass the host case.05 pins the
changed source and build configuration;06 verifies matching hashes and builds
Linux, and07 passes the same case in the pinned image.

Unknown at the original cause, at the cleanup cause, and inside another cleanup
wrapper each retain the exact original error/capture/completion, no resolution
or install, the original local binding/live bytes and no stage row. Another
Commit is refused before its constructor runs. The definite control resolves
once and a later explicit real Content/Save/history Commit succeeds. These are
original outcomes supplied through the public Content producer boundary, not a
native SQLite I/O-fault or quarantine proof. The three original terminal unknowns
are not settled; explicit fixture teardown occurs only after owner stop.

Store is Disposable WAL/OFF; overlay is schema16 MEMORY/OFF. All tests use100s
stops, locked Cargo1.85.1, LAYERFS_CONSTRUCTION_WORKERS=1 and repository ARM64
inputs. Image is `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Linux Store files are container-local, never `/work`. Cache/timing are functional
only; no performance admission or new sample is claimed. Durable builds and
execution stays NOT_RUN — deferred by owner. Existing F8/F5 proofs are retained;
this corrects the newly identified nested-carrier gap without relabeling them.

Final all-target Storage/Daemon Clippy08, fmt09,695-file boundary10 and21 owning
guard tests11 pass. Production LOC: 164082 -> 164088 (delta +6), exact parent and
staged comparison in12. Core98665→98671; active55791→55797; reference65417,
excluded predecessor36325 and excluded integration6549 unchanged. The pinned
production counter and shipped-SQL classification are unchanged.
