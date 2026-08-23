# G5-B candidate comparison

Disposition: **`RETAIN_K64_F64`**. Values below use final Canonical-v2 widths, not the stale Canonical-v1 figures in the initiating prompt.

## Operation complexity

Let `N` be reference count, `W` the dirty CDC window, `Z` the ordinal suffix rewritten by a count change, `H` tree height, and `R` retained open-summary nodes.

| Operation | K64/F64 current | Exact Xet 3–9 persisted shadow | CD32–64 LayerFS shadow |
|---|---|---|---|
| Fresh build | `Theta(N)` work, hard bounded 64-wide frontier | `Theta(N)`; streaming tree frontier can be bounded, current Xet gap builder is not | `Theta(N)`, hard bounded 64-wide frontier |
| Same-count replacement | `O(W + H)`; observed 5,334 counter bytes | expected local; hard `Theta(N)` suffix if cut class changes/no rejoin | expected local; hard `Theta(N)` suffix |
| +1/-1 early/middle | `O(W + Z)`, worst `Theta(N)`; 196,375/100,763 counter bytes | expected local; hard `Theta(N)` suffix | expected local; hard `Theta(N)` suffix |
| Range read | authenticated path `O(H + returned bytes)` | smaller path but far more objects/live bytes | path remains within 105% model |
| Full reconstruction/materialization | `Theta(N + file bytes)` | same payload Big-O plus more objects/SQL | same payload Big-O |
| Open-summary merge | not applicable | expected logarithmic only; hard `Theta(R)`, repeated adversarial streaming quadratic | no open-summary proposal; fixed frontier only |
| Reopen authority | complete authenticated current graph | no authority improvement | no authority improvement |

## Exact 100-MiB topology/byte comparison

The final G4 fixture has 5,284 references and a current K64/F64 file mapping of 196,055 bytes in 86 mapping objects.

| Metric | K64/F64 | Exact Xet 3–9 | CD32–64 model |
|---|---:|---:|---:|
| Live file mapping | 196,055 exact | 235,363 formal minimum; ~271,002 iid; 370,809 every-cut | 196,055 minimum; ~197,415 iid; 201,975 maximum |
| Ratio | 100% | 120.05% minimum | 100–103.02% |
| Mapping objects | 86 | 663 minimum; ~1,185 iid; 2,643 every-cut | 86–173 |
| One-leaf authenticated path | 5,050 | <=2,666 model | <=5,210 |

Exact Xet fails the 205,857-byte live gate before internal nodes: `5,284*36 + ceil(5,284/9)*28 = 206,688`. It therefore cannot be a qualifying LayerFS mapping profile.

CD32–64 starts considering a domain-separated marker after 32 children and forces closure at 64. It keeps current maximum node widths and plausibly writes roughly 4.4–8.4 KiB on ordinary count changes, but no-marker/every-marker/grinded streams remain suffix-linear and can reproduce roughly current full-suffix work. It is an informative shadow arm, not a hard-10x candidate.

## Directory boundary

G4 did not isolate directory mutation and the in-memory directory currently clones its complete `BTreeMap`. No persistent radix implementation is authorized. A future diagnostic must use 1,000/10,000/100,000 entries and same-ID replacement, leading insertion, and leading deletion; insertion bases are 999/9,999/99,999 so the mutation actually reaches the named size. It must separate mutation/publication from complete verification and report entries/bytes cloned, pages/index/wrapper bytes, objects, SQL/BLOB work, RSS/Q, and both walls. The same-ID row must first assert that the replacement is not a no-op. A candidate opens only if mutation itself is materially costly.

## Admission conclusion

The current material objective is real for ordinary early/middle count changes, but no frozen candidate simultaneously provides the 10x rewrite result as a hard adversarial property, <=105% live/path bytes, bounded frontier memory, canonical history independence, and an acceptable migration story. K64/F64 remains authoritative.
