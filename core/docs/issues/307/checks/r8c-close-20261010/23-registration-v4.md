# R8c prospective registration

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

[Registration v4](../../../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v4.json)
freezes source `307618496aadd3900d412b9d5ea9eab4267293da` before any registered
invocation. It is derived from v3 by
[make_registration_v4.py](tools/make_registration_v4.py). Registration v3, its
FAIL 042 and the R8b outcomes are unchanged.

The twelve selections, their criteria, oracles, stops and order are those of
v3. What differs:

| Item | v3 | v4 |
| --- | --- | --- |
| Product tree `core/crates` | `2e0d94c8835e` | `10bd82154437`: user-namespace denial in the Sandbox (C-1), request maxima in the accounting (C-2) |
| Runtime | `2aaaf317…` | `fdd962b7…`, deploying `serial_low_water` 256 (C-4) |
| Daemon | `632b17ea…` | `f44900cf…` |
| Confinement controller | namespace creation recorded | namespace creation judged a failure |
| Volumes | two of R8b | fresh copies 19 and 20; empty shared-proof volume 21 |
| Gates | eight unmet | G01, G03 met; G02, G04, G06, G08 met as rescoped by owner decisions; G05, G07, G09 decided by the invocations |

The held-mount criterion is exactly as registered in v3: the sibling must
reach Unmounted and Gone, and the held Unmount must be the reversible `Busy` or
a complete Unmounted. It is not changed for the private-namespace arrangement;
the product is.

Timing attempts authorized: **0** (decision C-15). Final suites, Clippy, static
checks and tooling tests ran once at this source before the registration was
written and are cited, not re-invoked: host 275 binaries, Linux 275 binaries,
the only failures on each side being the two input-gated precondition tests
registered here.
