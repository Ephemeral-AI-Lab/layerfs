# R1d prefetch64: complete validation demand producer

> **Status: Frozen implementation contract; named exits PASS, full R1 open.**
> First parent: published `ad1d3514b08a000386a14109ed636ce2b5a6e251`.
> SC-03/05/07/08. Full R1, native/global/physical and R2–R7/#288 remain open.

## Selected replacement and exact limits

The common validator currently gathers all eligible existing parents and
positive non-root bound children in a `demanded Vec`. ValidationState then creates
a complete missing Vec, sorts/deduplicates that population and passes it to
`inode::lookup_many`, which owns answers, root indices and each level's demands
for that whole population. Existing D/B declarations can each reach65536 and
the raw union can reach131072. That population must not be moved unchanged to
scratch or hidden in a new manager; it does not fit the current native row class.

Replace that complete demand producer with fixed64 raw-occurrence waves in the
real shared validator. Reuse `ALLOCATION_CHECK_BATCH = 64`, an existing admitted
caller shape. No global count/body/memo/read/worker/cache/quota/timeout expands.
No new SQL, table/profile/canonical/Bridge tag, dependency or provider is selected.
The public generic lookup_many contract remains unchanged; this caller submits
at most64 serials, bounding its logical answer/index/frontier population by64.
Its internal Vec capacities are separate:64 singleton groups can own256 usize
slots from minimum-capacity rounding, alongside old/next levels and actual pages.

The existing ValidationState positive/absence memo, alias sites/base bindings,
graph seen/pending/reachability/unreachable/reference/count/touched/final-row/
release authorities are separate open gates. This slice makes no complete R1
memory, native heap/cache/RSS/protected-progress or speed claim.

## Source, admission and failure order

Keep exact scalar input checking, declared row checks, topology root loading and
fresh-identity checks in their current common order. Under a base table:

1. A complete scalar count/EOF pass walks headers and exact binding cursors with
   no demand population or prefetch grouped inode read. It checks aggregate names against
   declared allowance and eligible raw demands against twice that allowance,
   using checked arithmetic. Existing non-root parent eligibility uses the exact
   fresh source; each positive non-root bound child is a demand occurrence.
   Counts, source/order/cursor/completion/EOF refusals precede prefetch grouped base
   reads. The earlier fresh-identity check remains in place and may already have
   issued its existing <=64 grouped reads; their work and failure precedence are
   preserved. This gate does not promise zero reads for the whole validator.
2. Replay the same immutable source. In header parent/name order, submit eligible
   parent first, then its positive non-root children. At64 raw pending occurrences
   or exact final EOF, flush that wave. The first pass's totals and second pass's
   exact consumed totals and the fixed streaming replay transcript must match.
   Same-count serial/name/eligibility substitution is an error, never guessed
   adoption, refresh or prefix recovery. First-party sealed/borrowed immutability
   remains the supported precondition for all later semantic passes.
3. Filter already known positive **and absence** memo facts, sort/compact local
   duplicates in a fixed missing array, then call lookup_many once if nonempty.
   Consume each successful answer exactly and apply existing memo clear-at-limit
   semantics. No whole missing or result union survives between waves.

The global sorted-serial prefetch order becomes explicit source waves with
local sorted missing serials. Ordinary logical/canonical/semantic results are
unchanged. Between independent canonical/provider faults, the first encountered
failure now follows that declared wave order; do not claim old global fault
precedence or combine faults into a generic success. Every original typed failure
propagates with no retry, alternative algorithm or synthetic result. No data
acquisition moves outside the product operation or receives a benchmark warm
cache claim.

The recorded InodeReadWork page/read-wave prefix is charged even when lookup_many
returns an error, and attributed to the prefetch site before original failure
propagation. The unchanged inode/read.rs does not report unseen acquisition inside
a failing provider batch or all unparsed returned pages; those observations remain
unavailable and are not invented or called complete physical accounting. Attempted
prefetch telemetry is distinct from this recorded prefix. Binding/alias/cycle
logical-demand accounting stays at its existing semantic demand sites. Existing
branch-out-of-range no-leaf demand behavior is a disclosed separate quirk; tests
must not invent charges to force an equality or repair unrelated source here.
Claims/ordering/native owner custody follows the already proved abandonment and
cleanup composition. A failed source/reader cannot reach root construction.

## Fixed ownership and ordinary work telemetry

Select one private streaming BLAKE3 replay grammar, domain
`layerfs/validation-prefetch-replay/v1\0`, then scope.object32/rootserialBE8.
Each header contributes parentBE8/opaqueOrdinalBE8/bindingCountBE4/wireNB_BE8/
eligibleParentFlag1; each binding contributes nameLenBE2/full name bytes/childBE8
(None0). Append completion byte1 only after exact selected binding EOF and finish
matches(header). At exact header EOF append actualD_BE8/actualB_BE8/rawDemandsBE8.
These bytes are a private replay transcript, no wire/canonical/table/opcode tag.
The first-pass hasher ends before replay; retain only expected32 and one replay
hasher. Compare digest and totals before final tail lookup/return. Earlier full64
waves may already have read canonical data when a late mismatch is detected;
retain actual recorded work and abandon, with no root effect, retry or adoption.
Hasher/native layout is extra fixed ownership;1024 serial bytes is not total heap.

Use two fixed `[u64;64]` arrays with checked prefix lengths, one pending and one
missing. This is1024 bytes of serial element storage, distinct from the bounded
lookup_many output, canonical page owners, memo and provider decode/cache work.
No Vec capacity overshoot or full-population reserve occurs for these serials.

Add `ValidationWork.prefetch: ValidationPrefetchWork`, eight ordinary u64 fields:

| Field | Actual event measured |
| --- | --- |
| examined_occurrences | Eligible raw source occurrences consumed by replay |
| memo_hits | Occurrences already answered by positive/absence memo |
| local_duplicates | Not-memo occurrences compacted within the current wave |
| submitted_serials | Unique missing serials actually submitted, including failed attempts |
| lookup_calls | Nonempty lookup_many attempts issued |
| max_pending | Actual largest pending prefix |
| max_missing | Actual largest submitted prefix |
| max_answers | Largest actual successfully returned answer vector length |

These observations run for normal callers and return through the existing
ValidationWork/result boundary. They are actual product telemetry, not a test
hook, fake allocator/clock or expected-number counter. Use checked bounds and
existing saturating work accumulation conventions. At successful completion,
examined = memo_hits + local_duplicates + submitted; pending/missing/answers≤64.
An error preserves attempted/partial work rather than claiming successful totals.
Existing physical/site counters remain distinct from these occurrence counts.

## Ownership, expected results and exit gates

- C1 owner: validate.rs plus focused validate/prefetch.rs and validate/facts.rs
  extraction; unchanged memo/lookup behavior stays in facts, parent private
  reexports serve existing alias/cycle consumers. New public work type/reexports,
  new external validation_prefetch.rs and the affected fixed-wave assertion in
  filesystem_bounds.rs. Source-only module files remain <=999, entries <=200.
- C2 proof owner: new external actual StoreProvider proof only; no SQL/product
  edits. AuthenticatedObjects observer delegates both scoped/unscoped batch reads
  unchanged and records actual requested IDs/bytes and provider counters.
- Root: shared interface/Bridge authority, contracts/log/checklist/architecture,
  independent review, all Cargo/checks/count/commit/push/#287 ownership. All
  workers preserve others and run no Cargo. No new Codex chat/task messages.

Independent expected v1 roots/pages/bytes and semantic alias/star/permutation
models must pass. Include >64 raw parent/child union, duplicate positive and
absence reuse, low memo pressure, exact tail waves and fresh/root exclusions;
count/site conservation, maximum actual call/answer bounds and fixed arrays;
late first-pass total/source failure before prefetch grouped reads, with earlier allocation
work preserved; original provider failure with
actual partial read/site attribution and terminal custody. Update the obsolete
height-only batched lookup assertion to its declared bounded-wave law, preserving
its other assertions rather than weakening unrelated limits.

Use actual System/Store observations in a declared window with fixture/setup
outside that observation, naming live memo/read/decode owners. Heap-only or
single-largest allocation cannot establish physical/global admission. All
physical read changes are reported as count diagnostics, not speed PASS. No
benchmark row/runner/family/campaign/Family2 rerun or #288 edit.

Freeze coherent source and run meaningful owning locked C1/Store/Server checks,
examples/fmt/Clippy/boundary/self-tests once, preserving every failure/correction.
Count exact first-parent/final staged/committed production scope and publish the
checkpoint only on actual exits; full R1 remains unchecked. Linux/native guard,
remaining graph authority and #288 qualification stay explicitly unrun.

## Reachable actual-provider failure proof correction

The first external malformed-inode fixture labeled as WholeFile was refused by
Store's role validation before the intended body. That original failed attempt
is retained. A structurally malformed authenticated inode is not supplied by
this Store route; the C1 external identity/canonical checks remain separate.

The corrected real-provider case saves valid independently framed objects,
closes Store, mutates one exact owned metadata_value_groups digest byte through
an external connection with affected-row1 guard, closes that connection and
reopens the actual Store. Source `encoding/full.rs` and `value_group` plus the
existing metadata-pool corruption proof establish the boundary: filesystem root
and inode branch remain ordinary; the pooled leaf batch returns original
`ProviderFailure("value group identity")`. Exact trace/attempt64/recorded root
page1/readwave1, claim failure and root denial are the expected result. There is
no product hook, provider substitute, retry, role bypass or native global proof.
Previously passing wide/refusal cases remain unchanged and their evidence reused.
