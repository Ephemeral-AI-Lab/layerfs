# R1d-binding-input: real scalar Server construction

> Scoped input/lookup/composition exits PASS; full R1 remains PARTIAL/open.
> Parente977700173af331daf9c92ae9931b1aab7d7775e. Actual commit/recount/push
> and #287 checkpoint follow the counted source; no future SHA is guessed here.

The real Server prepared route now streams one checked full name at a time into
private RowSpoolformat2. The unchanged48-byte header/32-byte slots select exact
counts, spans and roles; sparse16 full-name offsets replace directory-prefix
replay, while duplicated parent/count/inode/fresh payload fields are retired.
The prospective plan preserves every valid previously fitting body+1MiB shape
and executes before scratch/Save/body effects. Global wireEOF precedes seal;
scalar headers/cursors then serve subject checks, semantics and the existing
canonical builder. No caught-error whole-row fallback or quota increase.

C1 keeps at most32 scalar parent headers and one active directory cursor; exact
completion is checked for empty/new/maintained/unreachable and ordinary merges.
Alias lookup distinguishes Unmentioned/Absent/Present, follows changed sites
once, and effective cycle entries merge one bounded base page with a cursor.
The complete changed-name map, per-parent cloned name vectors and whole base/
effective-entry vectors are retired. Graph facts/frontiers/count/final-row
populations still remain and prevent full bounded-memory qualification.

The [concrete freeze](R1D-BINDING-INPUT-FREEZE.md) records selected descriptors,
independent public checked iterator completion and explicit compatibility. An
opaque descriptor belongs to its source; exact physical indices remain in
Spool/Slice, and old point sources use parent keys without prefix recovery.
First-party legacy point queries delegate borrowed/indexed producer access;
external old implementations retaining full-row lookup remain explicitly
unqualified for bounded point work. No new resident name/position cache or index.

## Owning proof and exact scope

[01 frozen source](evidence/r1d-binding-input/01-FROZEN-SOURCE.json) pins the
original coherent candidate and all worker responsibilities. Three trivial
wrapper/public lifetimes were elided after05 Clippy; common borrow semantics
and all functional behaviors are unchanged. [06 correction](evidence/r1d-binding-input/06-LINT-SOURCE-CORRECTION.json)
retains this change; original05 FAIL stays on disk. Final changed-source/test
seal7aab0034cda68d86cb8bc24c69472c9b7bc13aea6eaa13de4338b61b30e8022e
uses sorted changed owned core/crates paths, NUL, exact staged bytes, NUL.

Selected commands, all locked1.85.1 from owned worktree/root ARMv8 config:

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-bridge --test prepared_bindings --test prepared_stream --test history_protocol -- --nocapture
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_binding_rows --test filesystem_binding_validation --test filesystem_rows --test filesystem_topology --test filesystem_updates --test filesystem_bounds --test filesystem_reference --test filesystem_hardlinks -- --nocapture
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test prepared_bindings --test history --test catalog_admission -- --nocapture
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-content -p layerfs-server --all-targets -- -D warnings
cargo +1.85.1 build --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-content -p layerfs-storage -p layerfs-server --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p test_check_product_boundary.py
```

[02 Bridge](evidence/r1d-binding-input/02-bridge-tests.log)30 PASS,
[03 Content](evidence/r1d-binding-input/03-content-tests.log)75 PASS,
[04 Server](evidence/r1d-binding-input/04-server-tests.log)41 primary PASS plus
one empty-env helper excluded:146 primary checks. Real-file rows11, scalar
validation8, actual Stage6 cover boundary/fullname255/replay/tombstone/foreign/
earlyfinish/corrupt-checkpoint/truncate/EOF/refusal/admission and alias/cycle facts.
Independent sealed reference2 pins unchanged v1 roots/pages separately from
semantic/work evidence. Wide1024 common build route deliberately refuses whole
row API; accepted4088/4096/4097/4120 regressions pass. Actual14000-fresh spool
fits unchanged W+1MiB and reads exact typed rows; arithmetic alone was not PASS.

Original05 Clippy exits101/three needless wrapper lifetimes; only that equivalent
correction was made and07 all-target Clippy passes. Unchanged functional PASSs
reused, not rerun.08 examples compile only;09 whole-Core fmt passes, product
boundary399/selected guard9 PASS; unaffected SDK2/root counter checks reused.
No retired preflight/CI/benchmark, full final Core or Linux/physical qualification.
Complete command walls are correctness/build observations, not product speed.

## Count/resource/failure evidence

Independent v1 wire bytes prove aggregate name/byte budgets before sink effects,
strict order, compact root, per-role/fresh refusal before identity callback and
exact globalEOF. A local row completion cannot credit missing/surplus globalEOF.
The public scalar validator preserves exact sticky completion refusal;129
permuted directories produce258 alias visits and258 changed-name probes, with
at most2 simultaneous scalar cursors. No generated wide-root claim from candidate.

Real4097 name255 point reads check logarithmic full-name checkpoint probes and
at most16 local decodes, exact application read-byte accounting, and alternating
ordinary/legacy indexed APIs. Legacy257-directory header selection opens zero
sequences;257/1024-name repeated/descending/alternating-parent borrowed lookups
clone zero complete rows and open zero sequences. These are application work
facts, not kernel physical I/O/residency/cache measurements.

Direct legacy Service/Store/catalog proves exact names/references/unchanged base
bytes/Stage facts at15/16/17/127/128/129/1024/4088/4096/4097/4120; native spool
exists during receive and is removed on success. Name-only receive between
first read andEOF reports maximum single Rust allocation12 or255 bytes, inside
prospective16KiB. Observer/caller fixtures, later graph/SQL/native/workers are
excluded. At4120:body90653 bytes/12364 reads/8240 allocation calls/max12.
This does not prove whole-Stage peak/global/physical memory or native startup.

Known malformed count/order/namewidth/EOF and actual native truncate79/slot
offset56→1 preserve the prior exact Stage and yield known InvalidInput before
publication. Empty declaration uses acknowledged-slot presence pre-seal and
real empty canonical page. Typed C2/C5 admission/real2-Save catalog progress
and real scratch Unknown/known-Save abort (including failed cleanup) continue
to pass under changed mechanism; active slots/retained capsules/Branch/prior
finished independent roots preserve exact custody. The native executable's
Apple32MiB refusal is separate [R1e capability](R1E-BOOTSTRAP-DELIVERY.md).
Direct logical library tests cannot become a native executable/strict PASS.

## LOC, remaining authority and next

Production LOC:139582 ->141630 (delta+2048).
Reference:65417 ->65417 (delta+0).
Core:74165 ->76213 (delta+2048).
Same root counter blob a1cb6c064dd150dbc3aa19648a3748b5aeb741b5,
SHA2560d798f53263c0f18636d96ccdfdc3728f501f53b066e90577d251859f1f117b2,
on exact first-parent/final staged product Git archives; runtime package/src SQL
included, inline/test-only/test/example/fixture/tool/doc/manifest/generated/
third-party excluded. [Per-file LOC/source seal](evidence/r1d-binding-input/PRODUCTION-LOC.json)
records checked Core subtree887125124e35dd39593dff92816debf58a108ad9.
Shared decoder/algorithm remains one implementation, not duplicate authority;
legacy reference untouched, physical ceilings separate. Committed archive
must reproduce staged totals before normal publication.

Next is a named R1d-graph state boundary: select one complete growing fact/
frontier authority and real consumer, typed page/order/seal/admission/proof
contracts first. Demanded-serial/additions/sites/base-facts/declared/seen/frontier/
unreachable/count/touched/final-row/release populations are not yet fully paged.
No old quadratic walk moves unchanged into SQL. Legacy native identity/partial
append/refund/Drop retry, protected/healthy engine/factory drain/global/cache/
physical/Linux and R2–R7 remain open. #288 qualification delegated/unrun;
no campaign/family/runner/issue edit/speed/release admission.
