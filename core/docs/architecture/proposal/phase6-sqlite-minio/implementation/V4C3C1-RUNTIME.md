# V4c3c1 paged reservations runtime checkpoint

> Status: Research; informative and not a product contract.
> PARTIAL: owning checks pass; frozen live page-crossing gate next.

Parent50af45b896f4cdbf52c1519fa7dee722204174c5.
[Prospective contract](V4C3C1-SPEC.md), [actual check outputs](v4c3c1-native-checks/).
Experimental private P6META6, action7 exact owner/context/sequence/previous endpoint;
fixed8slot counters. Real C5reserve_inodes grants64serials; persistent singleton
owner context plus current grant only, no per-page lifetime registry/namespace scan.
Public Bridge/canonical/MinIO/C2/profile/worker/deadlines unchanged.

Bootstrap decodes owner and exactEOF, checks actual project/Branch and current
headNone before allocation. Actual C5scope/profile bound to incarnation. Publication
now checks the same owner. Each next request validates owner/scope/profile/sequence/
endpoint before allocation. Allowed interleaved allocator gap never overlaps.
Native client parses complete authenticated reply, exactEOF/range/cardinality and
context; uncertainty/malformed reply quarantines cursor. C5consumption never
refunded or replayed. Engine retains exact grant and independently checks each
refill; marks allocation pending before provider call, blocks subsequent creation
after refusal/uncertainty, leaves prior file bytes readable. No automatic retry.

Live Engine now creates from explicit first64grant and actual native provider,
refills before new inode/name mutation transaction only when exhausted. Unaffected
local Engine fixture constructor retains declared512serial grant. Terminal valid
signedC5range is admitted without overflowing the fixture constructor. This is
reservation paging, not larger population admission:512nodes/children,256handles,
changed/edit/verifier limits remain and inherited mount is explicitly refused.

27unique external owning checks PASS:7reservation,3engine,4metadata-session,
7construction,2directory,4rename. Actual C5/daemonSQLite: page crossing130created
files, interleaved gap, before-effects invalid owner/context/sequence/endpoint,
no replay, read-only allocation refusal/pending, no inode/name effect and retained
prior bytes, signed terminal grant, wireEOF. Real authenticated native fixture
consumes an actual C5grant then returns bad context; client quarantines with2actual
action7calls, no resend/refund, highwater remains194. The fixture proves native
client custody; actual service/daemon/kernel/provider composition awaits live gate.

Initial external test failed to compile because open_read_only also requires actual
binding key/cursor key; corrected call from source signature, no runtime hook.
Initial5reservation checks rerun only after source-evidenced independent Engine
refill validation/quarantine change;6covering checks then new native test individually.
No unchanged passing suite repeat to select speed. Host locked all-targetClippy/fmt,
native release and Linuxmuslrelease PASS. RootARMv8flags retained. Exact commands:

```
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --test reservations --test engine --test metadata_session
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --test reservations --test construction --test directory --test rename
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --test reservations native_range_reply_is_exact_and_bad_reply_never_resends_consumed_grant
cargo +1.85.1 clippy --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/benchmark/phase6-live/Cargo.toml -- --check
cargo +1.85.1 build --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release
cargo +1.85.1 zigbuild --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release --target aarch64-unknown-linux-musl
```

Full unchanged Core suites/examples/boundary/LinuxClippy unrun for tool-only change;
prior unaffected namespace/span/source/pack proof retained by scope. No CI/preflight,
WAL/sync/durability or physical observation claim. All seven owning families/genuine
SDKInit/fullDeepSeek/inherited import/population/Unknown/canonical/resource scope
open. Current next: frozen full128+sparse once-per-source live cohort crossing native
64serial page boundary, exact action7 counts, full byte/history/cleanup. Production
LOC comparison for this checkpoint is in its counted commit message/issue; tools/
tests/docs excluded consistently, shipped product unchanged.
