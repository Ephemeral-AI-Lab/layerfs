# P1-14 receipt — the whole-file edit assembles into the object's own buffer

> **Status:** Landed. Commit `d6bc1404a` (tree of this round's `after/` arm);
> before arm [`../p1-7/after/`](../p1-7/after/) — `666691c97`, the commit P1-14
> builds on. Artifact identities: [`after/artifacts.txt`](after/artifacts.txt).
> Elapsed figures are **diagnostic only**; no box is gated on one.

## 1. The item, as planned

The whole-file arm built the payload in `out`, `encode_whole_file` built the value
around it, and `encode_bytes_object` built the canonical object around that — three
live allocations of the same bytes, plus `FileView`'s own copy of the base
canonical. V2's memory probe measured the peak delta at **262,328 = 4n + 184** for
n = 65,536 (the plan's `~3n` estimate missed the view's copy).

Target: assemble into one pre-sized buffer; gate `peak_delta_bytes` **≈131,118**
(the "≤ ~2n" bound restated by the continuation handoff).

## 2. What landed

`begin_whole_file_object(capacities, payload_len)` (`file/content.rs`) sizes the
canonical object itself: both capacity checks with today's `what` strings and
limits, `try_reserve_exact(canonical_len(WHOLE_VALUE_HEADER + payload_len))`, then
the 13-byte envelope and the value header. The pre-sized buffer **is** the sink —
`assemble_into` appends the retained ranges and the replacements into it — the
final check is `canonical.len() == HEADER_LEN + VALUE_LEN_BYTES +
WHOLE_VALUE_HEADER + final_len`, and `FinalizedObject::new` **moves** it.
`encode_whole_file` stays for complete construction and is now also the reference
the new test compares against.

## 3. The measurement

`edit_memory_probe` (counting allocator, one sample, deterministic input):

| Row | Before (`666691c97`) | After (`d6bc1404a`) | Predicted |
| --- | ---: | ---: | --- |
| `peak_delta_bytes` | **262,328** | **131,826** | ≈131,118 ✔ |
| `peak_live_bytes` | 394,503 | 264,001 | lower ✔ |
| `baseline_live_bytes` | 132,175 | 132,175 | unchanged ✔ |
| `edited_root` | `8ddfe36c…` | `8ddfe36c…` | identical ✔ |
| `objects_written` / `_bytes` | 1 / 65,559 | 1 / 65,559 | identical ✔ |
| `nodes_read` | 1 | 1 | unchanged ✔ |

**The arithmetic, not a rounded claim.** The after figure is `2n + 754`: one
canonical buffer of `n + 23` (131,095 with the reservation's slack) plus the
probe's own 512-byte replacement buffer and the fixture's live source. The
predicted 131,118 was `2n` exactly; the 708-byte difference is stated here rather
than subtracted from the ledger. Before, the same probe held `4n + 184`.

## 4. Parity

* The **whole counter-only diff** between this arm and `p1-7/after` is two lines:
  `peak_live_bytes` and `peak_delta_bytes`. Every D-row, every other M-row, both
  X-rows and every emitted root and canonical byte count are identical.
* New test `whole_file_edit_emits_the_reference_bytes`: the emitted object equals
  `encode_whole_file(capacities, expected)` byte for byte. It **passes on the parent
  tree as well** — it is the byte-identity oracle for this route, not the gate; the
  probe is the gate. Recorded that way rather than presented as a falsification.
* The whole-file route still reports `EditCounters::default()`, which
  `edit_transitions.rs:771` pins; nothing was charged to this route.
* Every emitted root and byte count on the frozen set is unchanged, including D10's
  65,559 canonical bytes and D20's byte-for-byte readback.

## 5. Honest gaps

1. **The timing tree loses the `content.encode` child on this route.** The
   assembly and the framing are one pass now, so there is no separate encode
   scope. No test pins it there; stated rather than re-added as a no-op scope.
2. **Complete construction (`construct_bytes`) is unchanged** and still pays the
   value-plus-canonical double allocation. That is out of this item's scope (the
   plan says so explicitly).
3. **`peak_delta_bytes` is a requested-bytes figure from a counting allocator, not
   RSS.** It is deterministic for a fixed input, which is what a before/after
   comparison needs; it is not a claim about resident memory.
