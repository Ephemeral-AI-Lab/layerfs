# Prospective G4 materialization acceptance v8

Status: frozen before any v8 preparation or measured row.

V8 is the isolated RSS repair. V7 is preserved as a complete 30-record / 50-arm durability-correct campaign whose only terminal issue was one closure-on child at 22,020,096 bytes RSS, above the 20,971,520-byte hard cap. V8 imports no v7 row and performs no selective rerun.

The candidate keeps every v7 correctness, reconciliation, descriptor, cleanup, counter, seed-label, and lock-lifetime repair. It adds only a G4 read/materialization connection cache cap:

- R0 immutable current control and scalar M0 control retain 2,000 SQLite cache pages;
- same-binary R1 attribution/candidate, R1 fresh, and batched M0 candidate use 1,500 pages;
- both R1 A/B arms use the same 1,500-page setting, so closure folding remains their only work variable;
- `cache_spill=2000`, synchronous FULL, rollback journal DELETE, temp_store=FILE, mmap=0, schema, Canonical-v2 identities, FastCDC, fixed radix, writer paths, G3 routes, and protected controls remain unchanged.

The 500-page reduction creates approximately 2 MiB of deterministic process-RSS headroom without a new buffer, dependency, cache manager, or persistent state. Every G4 row reports and both analyzers gate its observed `sqlite_cache_size_pages`.

Frozen custody:

- Repository / branch / HEAD: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`, `codex/empty-worktree`, `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable: `c60a19cb3cecb83bb801ba9c36835297e6fc503d736171213ec78e69bd5d6d76`.
- G3 control: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Protected control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.
- Round-1 handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`.
- Rust sources: benchmark `eb00674125d18da66253b31949ecba2f874b64ec6a93ad68fe251d4f0649d169`; G3/G4 module `32c8185c3cbc5b444ba0a533ea5f1bd9332b16eb358b9c5540c0ab534ac3f8d9`; unchanged Canonical-v2 `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`; Cargo.lock `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen v7 runner / primary / independent sources: `d3b6f7361cdaa549f5d6ad332fd93c768663c7375cb9c2cb7d6156faf922bbc7`, `f26966130fa8ced33c415feca8ee5f0423ec0b2b0dc7a409d397bdcd15f3b77b`, `0c0f897ea8a1f318afb6554829e6c74907565f6e5a46ed0de39ca26f182d010b`.
- V7 terminal / verification / wall / lock release: `6b508fbef268e8e3eabb200d29b408c44bafc8a641c60956e165a742e0596552`, `391c989b34a320e23b38e20d2aced6b1fd2d4257be01701c35804596e9578e6f`, `ac3b44245af5feacff61af31d34ccac63f76e787c4c5031fc8d09ba7b22b2716`, `df5b3d2465342883d55f63473d54b7ae00882ea90e30120c7e4bf2b15d2fdccb`.
- V8 result root: `target/phase4-g4-materialization-acceptance-20260822-v8/`, required absent before atomic `target/BENCHMARK_LOCK` acquisition and never reusable.

All v7 gates remain exact: M0 absent-prior old-or-new reconciliation and focused fault proof; checked writer and direct durability counters; descriptor-only verification; sequence binding; identity-bound cleanup and scanned residue; seed cache class; cleanup root `work-v8`; lock held through fsynced terminal verification and released with a separate attestation.

The unchanged 30-record / 50-arm protocol still requires R1 <=333 ms and >=5% direct improvement; fresh/M0 <=400 ms; seed no-digest <=50 ms; exact S1-100 SQL/row/BLOB/authentication/write shape; G3 semantic/Q/residue and 10/10/20-ms targets with relative inference Unavailable at n=1; protected full-create/edit/range/reopen <=5%; every whole child RSS <=20 MiB; buffers <=1 MiB; operation-local sum <=20 seconds; zero terminal Q/residue; two cold/physical-I/O Unavailable cells; independent ledger equality; 5/65/15/5/5/5/10-second buckets; and complete wall <=120 seconds.

Affected focused tests, clippy, fmt, diff, and release build pass before measurement. After measured PASS, one final workspace test closure and two fresh v8 read-only audits are required. G5 remains blocked, out of scope, and not started. No commit is authorized.
