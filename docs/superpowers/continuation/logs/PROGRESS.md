area-p 239/0 baseline green (+2 P7 groups, +1 P8 group, +1 budget traversal)
area-p 239/0 P9 v2 pre-change baseline with /usr/bin/node v24.22.1
area-p 239/0 P9 v2 implementation-session baseline green (plus 4 validation/traversal checks)
area-p 0/1 fake subscription-ID syntax error before cases; corrected for one rerun
area-p 239/0 rerun green after fake syntax correction (plus 4 validation/traversal checks)
area-p 239/0 continuation baseline green with /usr/bin/node v24.22.1 (plus 4 validation/traversal checks)
agent-event-private-smoke 1/0 exact public keys and fs.committed routing after WeakMap tightening
area-p9-events 13/0 all stateful fake scenarios green including >65519-byte stale snapshot
init-watchers 5/0 focused groups green; fixed runtime.close InitJobs ownership ReferenceError
init-watchers-real 1/0 real Linux Disposable Init: 9 events, outer phases complete, imported scan/files/publish, zero host get_init reads
runtime/surface 1/0 subscribe_init surface present and existing lifecycle checks green
runtime/worker-frames 1/0 frames plus disconnect, late subscribe, old-ID refusal and no-backlog checks green; fixed missing op destructuring
native_events 2/0 real Disposable Store mappings/lifecycle and UV=2 long-poll admission green
Rust clippy layerfs-node initial 0/1 -D warnings identified non-snake-case local hasMore; wire field retained and local renamed
area-p final 239/0 /usr/bin/node v24.22.1 (plus 4 validation/traversal checks)
calls final 64/0 /usr/bin/node v24.22.1
cli.test final 104/0 docs tables and no-events CLI surface green
area-p9-events final 13/0 UTF-8 fake valueCost and >65519-byte stale snapshot green
Rust layerfs-api test build initial 0/1 --tests --no-run hit 120 s while compiling; no test executed
Rust layerfs-api aggregate test build second 0/1 same compile-only timeout; switched to required per-target builds, no tests executed
Rust P1/P7/P8 target builds 3/0 locked offline --no-run completed individually
packaging final 8/0 surface lifecycle options copy sandbox worker-route transport-order dispatch
init-watchers final 5/0 focused watcher groups green
init-watchers-real final 1/0 real Linux Disposable Init 10 events imported files/publish zero host get_init reads
runtime/surface final 1/0 subscribe_init and existing lifecycle green
runtime/worker-frames final 1/0 disconnect late-subscribe old-ID and no-backlog green
Rust P14/golden target builds 2/0 locked offline --no-run completed individually
native_events final 2/0 real Disposable Store mappings/lifecycle and UV=2 admission green
area_p1_history final 4/0 real Disposable neighbor history green
area_p7_attribution final 8/0 real Disposable attribution green
area_p8_reads final 1/0 real Disposable reads green
area_p14_paging final 6/0 real Disposable paging green
sdk_method_golden final 1/0 umask 022 golden green including events wire methods
product-boundary final 1/0 967 production files scanned
Rust clippy layerfs-node final 1/0 --all-targets -D warnings green; hasMore wire keys unchanged
rustfmt initial 0/1 three formatting diffs in native_events.rs; corrected for final rerun
rustfmt final 1/0 cargo +1.85.1 fmt --all -- --check green
native_events post-format final 2/0 rebuilt target and reran green
Rust clippy layerfs-node post-format final 1/0 -D warnings green
events_real syntax 1/0 node --check green; Docker/FUSE execution NOT RUN here
sdk_method_golden scope correction 1/0 recorded Store methods only; P9 events verified by area-p9-events/native_events and protected golden file unchanged
area-p9-events P9 v2 REVISE regression-first 13/0 fake positive-poll admission saturation assertion green
native_events P9 v2 REVISE regression-first 0/1 explicit-null poll assertion reproduced unintended success before fix; 2 filtered
native_events P9 v2 REVISE intermediate 2/1 null fix and 1002-branch snapshot green; undefined-field assertion corrected because direct wire omits undefined
area-p9-events P9 v2 REVISE final 13/0 explicit fake-wire positive-poll admission saturation green
native_events P9 v2 REVISE final 3/0 null/undefined-absence semantics, real addon mappings/admission and 1002-branch two-page snapshot green
area_p1_history P9 v2 REVISE final 4/0 locked offline target build then 120 s bounded run green
area_p7_attribution P9 v2 REVISE final 8/0 locked offline target build then 120 s bounded run green
area_p8_reads P9 v2 REVISE final 1/0 locked offline target build then 120 s bounded run green
area_p14_paging P9 v2 REVISE final 6/0 locked offline target build then 120 s bounded run green
sdk_method_golden P9 v2 REVISE final 1/0 umask 022 locked offline target build then 120 s bounded run green
rustfmt P9 v2 REVISE final 1/0 cargo +1.85.1 fmt --all -- --check green
Rust clippy layerfs-node P9 v2 REVISE final 1/0 --all-targets -D warnings green
product-boundary P9 v2 REVISE final 1/0 967 production files scanned
product-boundary-self-tests P9 v2 REVISE final 155/0 tools unittest discovery green
real-addon-null-probe P9 v2 REVISE final 3/0 cursor/timeoutMs/limit explicit null each InvalidInput with exact validation message
area_p1_graph retro-f3 pre-change main 0/1 intended bounded-work failure: rows 100->200 = 416->824, statements = 828->1640; page/publication assertions passed
area_p1_graph retro-f3 pre-change branch 0/1 intended bounded-work failure: rows 100->200 = 125->229, statements = 139->245; direct-source/absorbed-parent flags passed
area_p1_graph retro-f3 implementation 2/0: main rows/statements flat 196/212 at 100 and 200 Layers; branch flat 20/38; current publication and absorbed-parent flags green
rustfmt retro-f3 initial 0/1 new-test layout only; cargo fmt applied; final 1/0 cargo +1.85.1 fmt --all -- --check green
area_p1_graph retro-f3 final 2/0 post-format locked offline build then bounded run green
area_p1_history retro-f3 final 4/0 locked offline build then bounded run green
area_p14_paging retro-f3 final 6/0 locked offline build then bounded run green
area_p7_attribution retro-f3 final 8/0 locked offline build then bounded run green
sdk_method_golden retro-f3 final 1/0 umask 022 locked offline build then bounded run green
clippy retro-f3 final 1/0 workspace exclude layerfs-node all-targets locked offline -D warnings green
product-boundary retro-f3 final 1/0 967 production files scanned
product-boundary-self-tests retro-f3 final 155/0 tools unittest discovery green
area-p retro-f3 final 239/0 plus 2 P7 groups, 1 P8 group, 1 budget traversal green
calls retro-f3 final 64/0 green
cli.test retro-f3 final 104/0 green
