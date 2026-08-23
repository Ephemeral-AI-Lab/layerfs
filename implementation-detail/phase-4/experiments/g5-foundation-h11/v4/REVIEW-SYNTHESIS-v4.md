# G5-0 v3 static-failure repair synthesis

Fresh root-cause and custody lanes agreed that v3 must remain frozen and v4 must receive a new source/executable/freeze. The accepted repair makes the already-used `open_measured` carry optional phase output and performs no clocks on `None`. This avoids a lint allowance, dummy caller, public API, or production behavior.

The v4-only H11 refactors address both isolated clippy findings without semantic changes: a revision identity value replaces three scalar arguments, and build-time helper insertion precedes the retained G3 test module. V1/v2/v3 source and result artifacts remain historical custody.

