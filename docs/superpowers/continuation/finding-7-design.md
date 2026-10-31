# Retro finding 7 design — late line windows byte cursor

Status: DESIGN ONLY, for commander review. No implementation is authorized by
this document. This is an additive SDK/CLI contract change, not a Store schema
or transport-codec change.

## Launch verification and current-source evidence

- Requested base: private `area-p/p1-p15` tip `6c9111ec1`.
- `/home/hatch/workspace/layerfs-area-p-work` is a source copy without `.git`
  metadata. `git rev-parse HEAD` there failed with “not a git repository,” so
  the commit tip could not be independently re-verified without entering the
  prohibited clone. The commander must retain tip verification as an
  implementation-launch precondition. The source locations below were
  re-verified directly in the supplied product tree.
- `engine/crates/layerfs-store-service/src/read/window.rs:128-191` opens a
  `FileView` and calls `view.read_range(reader, 0..size, ...)`; the `Feed`
  starts at offset zero.
- `window.rs:309-384` counts and discards lines before `from`; it does not
  retain byte positions at line boundaries.
- `engine/crates/layerfs-store-service/src/read/reads.rs:27-109` exposes only
  numeric line starts and `nextLine`.
- `engine/crates/layerfs-store-service/src/read/cached.rs:1-27` deliberately
  scopes storage caches to one call. This design does not change that lifetime.
- Existing P8 coverage has huge-line early-stop cases but no sequential
  many-line cursor traversal or two-size content-work proof.

For a file of `B` bytes and windows of at most `W = 48 KiB`, numeric sequential
reads currently process `O(B²/W)` content bytes. The proposed cursor sequence
must process `O(B)` content bytes, with only bounded rescanning of an
unreturned line and ordinary per-page snapshot/path lookup work. This is a
content-processing bound, not a claim that all metadata work disappears.

## Contract summary

Add an optional request field `offset` and response field `nextOffset` to
`fs.read`:

- `offset` is an optional versioned string cursor. `null` or omission means no
  cursor and preserves numeric line-only behavior.
- `nextOffset` is always present in a successful reply and is either a string
  cursor or `null`. It is non-null exactly when `nextLine` is non-null.
- The cursor identifies the first byte of the unreturned line named by
  `nextLine`. Passing it back starts the content range at that byte instead of
  byte zero.
- `startLine` remains supported. A valid cursor takes precedence over a valid
  numeric `startLine`, following the existing difference-cursor precedent.
- No `nextCursor` field is added to `fs.read`; that name continues to mean the
  path keyset cursor on difference operations.

The line window, 48 KiB reply budget, 2,000-line cap, `endLine`, `text`/`bytes`,
`lineTruncated`, `truncated`, `nextLine`, `size`, and snapshot fields retain
their current meanings.

## Cursor format and binding

### Wire representation

The cursor is an ASCII string with this exact version-1 representation:

```text
lfr1.<mode>.<line>.<byteOffset>.<contentRootHex>.<pathHex>
```

Fields are:

1. `lfr1`: literal format/version prefix.
2. `mode`: `t` for text mode (`binary: false`) or `b` for binary mode
   (`binary: true`).
3. `line`: the one-based `nextLine`, canonical unsigned decimal.
4. `byteOffset`: the zero-based byte offset of that line's first byte,
   canonical unsigned decimal.
5. `contentRootHex`: exactly 64 lowercase hexadecimal characters encoding the
   file's immutable `content_root` `ObjectId`.
6. `pathHex`: nonempty, even-length lowercase hexadecimal encoding of the
   normalized absolute path's UTF-8 bytes.

Generation uses canonical decimal and lowercase hex. Parsing rejects unknown
versions, wrong field counts, empty fields, noncanonical numbers, invalid or
uppercase hex, invalid UTF-8 paths, paths that fail the existing path
validator, unsafe-integer line/offset values, and other malformed encodings.
Callers must nevertheless treat the string as opaque and must not construct or
edit it; the explicit representation exists to make the contract and refusal
tests deterministic and to permit future versioning without a codec change.

For example, a text cursor to line 2 at byte 6 of `/file.txt` has the form
`lfr1.t.2.6.<64-lowercase-hex>.2f66696c652e747874`.

### Binding rules

After resolving the snapshot and looking up the requested path, the reader
validates the cursor against all of the following:

- normalized request path equals `pathHex` decoded as a path;
- current file `content_root` equals `contentRootHex`;
- requested mode equals `mode`;
- `byteOffset <= size`, with the existing safe-integer limits also enforced.

The cursor is bound to immutable file content, not to a Commit/Layer identity.
Consequently:

- **Another Commit or Layer, same normalized path and same content root:**
  accepted. Snapshot identity may differ; the continuation is still exact.
- **Advanced Branch head, same path and unchanged content root:** accepted.
- **Changed file content:** refused because the content root differs.
- **Metadata-only change with unchanged content root:** accepted; file bytes
  and line positions are unchanged.
- **Another file:** refused because the normalized path differs, even if the
  other file has identical content.
- **Missing path at the other snapshot:** the existing `NoSuchPath` refusal
  occurs during lookup, before cursor validation.

These rules deliberately bind both content and path. Content-only binding
would make a cursor from an unrelated but byte-identical path silently valid,
which is surprising for a path-scoped `fs.read` operation. Omitting the Commit
identity avoids refusing safe continuations across snapshots that share the
same immutable file content.

A cursor is an opaque continuation token, not a security capability or an
authentication mechanism. As with existing path keyset cursors, a caller can
submit a syntactically valid position that the server did not issue. The
classification guarantee below applies to chains of server-issued cursors;
content/path/mode checks prevent accidental cross-read reuse, not malicious
fabrication.

## Refusal semantics and precedence

### Precedent

`engine/crates/layerfs-store-service/src/read/compare.rs:322` sets numeric skip
to zero when a difference cursor is present: cursor wins over `pagePos`.
`engine/crates/layerfs-api/src/base/args.rs:53-59` still validates paging
arguments and rejects malformed cursor input as `InvalidInput`. Finding 5
extended the same behavior to `changed_paths`. This design mirrors both parts.

### Exact rules

1. Both `offset` and `startLine` may be present. A valid `offset` selects the
   effective start line and byte range; a valid numeric `startLine` is then
   ignored for positioning.
2. Numeric arguments are still syntactically validated. Thus `startLine: 0`
   remains `InvalidInput` even with a cursor, just as invalid `pagePos` is not
   hidden by a difference cursor.
3. The reply's `startLine` is the effective cursor line, not the ignored
   numeric argument.
4. `endLine` remains an absolute line number. It is compared with the effective
   start line. If it is smaller, retain the exact existing refusal:
   `InvalidInput`, `endLine must be at least startLine`, null detail.
5. `offset: null` and omitted `offset` both mean numeric line-only operation.

Use these exact message classes, both with null detail:

| Condition | Code | Message |
| --- | --- | --- |
| Wrong wire type, malformed encoding, unsupported version, invalid number/path | `InvalidInput` | `offset is not a valid read cursor` |
| Path, content-root, mode, or file-size binding mismatch | `InvalidInput` | `offset does not match this file read` |

Existing snapshot and path refusal ordering remains unchanged: snapshot/path
resolution happens before line/cursor arguments are consumed. Therefore a
missing path still wins over a malformed or mismatched cursor. Existing
`Binary`, `IsDirectory`, `NotFile`, Branch, Commit, and Layer refusals are
unchanged.

A cursor is not silently ignored when it is stale or belongs to another file.
There is no fallback to numeric start after a cursor error, and no automatic
retry.

## Boundary and continuation semantics

The scanner must track the absolute byte offset at which each logical line
starts, in addition to its line number and byte length.

- Offset zero is the start of line 1.
- After a line containing `n` bytes including its LF, the next line starts at
  `currentOffset + n`.
- A final LF belongs to the preceding line and does not create a phantom line.
- An unterminated final line has no continuation after it.
- `nextOffset` always points to the beginning of the first unreturned line,
  never into a returned line and never merely to the scanner's current read
  position.
- If the scanner inspected part of an unreturned line before stopping, the
  next cursor rewinds to that line's start. That bounded overlap is required
  for exactly-once output and correct truncation/classification.
- A returned truncated line is still fully scanned under the current rules;
  if another line follows, its cursor points after the complete returned
  line's terminator.
- `nextLine: null` requires `nextOffset: null`; `nextLine: n` requires a
  non-null cursor whose `line` is `n` and whose `byteOffset` is that line's
  first byte.

For cursor calls, initialize the scanner's open line to the cursor's line and
offset, and call `read_range` for `byteOffset..size`. `Feed.from` and the
16 KiB classification-window calculations continue to use absolute file
offsets, not suffix-relative offsets. Whole-file in-memory payloads may be
sliced at `byteOffset`; chunked views must request only the suffix. No
cross-call read cache is introduced.

## Text-mode Binary classification preservation

Mode binding is mandatory. A text cursor cannot be used with `binary: true`,
and a binary cursor cannot be used with `binary: false` or an omitted `binary`
flag; both are `InvalidInput: offset does not match this file read`.

For a server-issued text-cursor chain:

1. A page is returned only after the existing text scanner has accepted all
   content needed for that page. If a NUL or invalid UTF-8 is encountered in an
   earlier page or its scanned prefix, that earlier call returns `Binary` and
   issues no cursor.
2. Therefore a successful text cursor certifies that all bytes before its line
   boundary have already passed the existing classification algorithm. The
   next call need not classify that prefix again.
3. The cursor is at an LF line boundary in validated text, so it is also a
   UTF-8 character boundary. No partial UTF-8 sequence needs to be carried
   between calls.
4. The suffix scan retains absolute 16 KiB window numbering and the full file
   size. Binary behavior cannot change merely because storage chunks or read
   blocks begin at a different suffix offset.
5. Any NUL or invalid UTF-8 in a later page still returns the exact existing
   `Binary` refusal. A binary-mode first page can never be converted into a
   text continuation that skips an unclassified binary prefix.

Binary mode performs no UTF-8/NUL classification, as today; its `b` cursors
are valid only for subsequent binary reads of the same path and content.

## Public surface changes

### Rust wire/API surface

`fs.read` accepts these fields in addition to the current fields:

```text
offset?: string | null
```

Successful replies add:

```text
nextOffset: string | null
```

`engine/crates/layerfs-api/src/fs/read.rs` parses the optional field with the
same lazy-argument discipline as line arguments and passes it into the Store
read service. The service resolves it to an effective line/byte start and
returns the continuation beside `nextLine`.

The public Rust function signatures may use a small typed cursor/start value
internally, but no new public SDK method, Store schema object, transport tag,
or `SdkValue` variant is introduced. Node's generic `SdkValue` conversion
already supports text/null, so `layerfs-node` requires no production change.

### JavaScript SDK

Keep the positional signature and extend the existing final options object:

```text
fs.read(ref, path, startLine?, endLine?, { binary?, offset? }?)
```

Validation behavior:

- `offset` may be `undefined`, `null`, or a string.
- `undefined`/`null` omits the wire field.
- Other types throw `LayerFSError("InvalidInput", "offset must be a string or null", null)`
  before any binding call.
- The existing options-object and unknown-key validation remains; the allowed
  keys become exactly `binary` and `offset`.
- The wire request includes `offset` only when it is a string. Existing calls
  retain their current request fields.

The JS result exposes `nextOffset` unchanged as a string or `null`.

### CLI

Extend `layerfs fs read` with:

```text
--offset <cursor>
```

The flag maps to `options.offset` and then the `offset` wire field. It combines
with `--start-line` using cursor precedence and with `--end-line` as an
absolute end. `--binary` must match cursor mode. Omission preserves existing
invocations. JSON and text output naturally include `nextOffset` when the
binding returns it; no special output encoding is added.

### Documentation and golden surface

Update the JS signature, CLI usage row, paging/read explanation, cursor
binding/precedence, mode-mismatch rule, and exact refusal messages in
`api/docs/agent.md`. Update `docs/general/sdk.md` with the additive read
continuation and the fact that numeric paging remains supported. Update the
SDK method golden replies so every successful `fs.read` includes
`nextOffset` (usually `null` in existing cases).

## Scaling and correctness test plan

All performance claims use the existing storage byte/read counters, not wall
time. The fixtures must be multi-pack files so the test exercises ranged
content reads rather than a single whole-file payload.

### 1. Two-size linear sequential traversal

Add to `engine/crates/layerfs-api/tests/area_p8_reads.rs` a helper and test
that run separately at `B = 2 MiB` and `B = 4 MiB`.

Fixture shape:

- ASCII lines of exactly 128 bytes including LF, with the zero-padded line
  number embedded in each line;
- no final partial line;
- enough lines to make exactly 2 MiB and 4 MiB;
- text mode, 48 KiB reply budget, default line cap not reached before the byte
  budget.

For each size, begin with an ordinary first read and then repeatedly pass
`nextOffset` as `offset` until `nextOffset`/`nextLine` are null. Around the
whole traversal, snapshot the existing `ContentIo` counters used by P8:
`pack_read_bytes + payload_read_bytes + range_acquired_bytes` and
`read_packs + payload_reads`.

Assertions for each size:

- Reassembled content equals the original bytes exactly.
- Every expected line appears exactly once, in order, with no repeats/skips.
- Each reply's effective `startLine`, `endLine`, `nextLine`, truncation flags,
  and cursor line agree.
- Before the final page, `nextOffset` is a non-null string; on the final page
  both continuation fields are null.
- The sequence terminates within the expected page count plus one defensive
  bound; it cannot loop on a nonadvancing cursor.

Cross-size assertions, recorded with actual counts in test output:

- `bytes(4 MiB) <= 3 * bytes(2 MiB)` and
  `reads(4 MiB) <= 3 * reads(2 MiB)`.
- Also assert a late-page bound: the final page of the 4 MiB traversal must
  read fewer than 2,000,000 content bytes. The old final-page prefix scan is
  over 3 MiB before storage overhead, while a suffix page plus bounded pack
  read-ahead remains below this limit.

The expected old cumulative shape is approximately quadratic (`B²/(2W)`), so
doubling file size multiplies prefix work by about four; the new shape is
linear plus bounded page/pack overhead. The ratio assertions alone are not
relied upon—the late-page absolute bound directly rejects a final prefix
rescan. No timing threshold is used.

**Old-code failure:** the first reply has no `nextOffset`, so traversal fails
for the intended missing-cursor reason. If driven numerically for a pre-fix
diagnostic, the old implementation also exceeds the late-page bound and shows
approximately 4x work at doubled size. That diagnostic is evidence only, not a
production-test fallback: the final test must never silently substitute
`nextLine` when `nextOffset` is absent.

### 2. Exactly-once coverage across line and reply boundaries

Add a focused real-Store test in `area_p8_reads.rs` with short lines arranged
around 16 KiB classification windows, 256 KiB scanner blocks, and the 48 KiB
reply budget. Include a returned truncated line followed by another line, a
final LF case, and an unterminated-final-line case.

Assert that cursor continuation is always the first unreturned line's start,
that the concatenated pages equal numeric one-shot/reference windows, and that
no phantom line is introduced after a final LF.

**Old-code failure:** no `nextOffset` is returned; cursor-driven continuation
cannot be performed. Existing numeric-only assertions alone would not
discriminate this test and are not sufficient.

### 3. Precedence and numeric compatibility

Add real-Store cases in `area_p8_reads.rs`:

- Obtain a cursor to line `n`, then call with both that cursor and a different
  valid `startLine`. Assert the reply starts at cursor line `n`, returns the
  same content as the cursor-only call, and ignores the numeric start only for
  positioning.
- Combine a cursor with an earlier `endLine`; assert exact existing
  `endLine must be at least startLine` refusal.
- Combine a cursor with `startLine: 0`; assert numeric validation still
  refuses.
- Run legacy line-only windows and assert every pre-existing field and content
  result is unchanged except for the additive `nextOffset` field.

**Old-code failures:** the precedence cases use the supplied numeric start and
return the wrong window because old code ignores `offset`; malformed combined
arguments likewise do not follow the new effective-start rules. The pure
numeric compatibility case passes on old code by design and is explicitly a
non-discriminating regression guard.

### 4. Cursor format and binding refusals

Add table-driven real-Store cases in `area_p8_reads.rs` for:

- non-string, empty, malformed, wrong-version, uppercase/noncanonical, bad
  path, zero/overflow line, and overflow/negative-equivalent invalid encodings;
- a cursor from another path with identical content;
- a cursor after content changes at the same path;
- text cursor with binary mode and binary cursor with text mode;
- offset beyond file size;
- another Commit/Layer with unchanged path/content, which must succeed and
  continue at the cursor line rather than restart at line 1;
- missing path at another snapshot, which retains `NoSuchPath` precedence.

Assert exact code/message/detail for every refusal.

**Old-code failures:** old Rust ignores `offset`, so malformed, stale,
cross-file, and mode-mismatch cases return ordinary numeric reads instead of
the required refusal. The valid cross-snapshot case restarts at line 1 and
fails its continuation assertion.

### 5. Classification preservation

Add targeted cursor cases in
`engine/crates/layerfs-api/tests/sdk_text_scan.rs`, reusing its window/block
reference corpus:

- Traverse selected valid UTF-8 files with characters and line boundaries
  around classification-window and scanner-block edges; compare every cursor
  page with the existing numeric reference.
- For files with NUL or invalid UTF-8 after one or more valid reply pages,
  assert early cursor pages succeed and the page reaching the bad window
  returns exact `Binary`; a complete cursor traversal must never succeed.
- Attempt to switch a binary cursor obtained before such bad content into text
  mode; assert the mode-mismatch refusal rather than classifying only a later
  suffix.

**Old-code failure:** cursor replies are absent and `offset` is ignored, so
cursor equivalence/continuation assertions fail. In particular, the mode test
does not produce the new required mismatch refusal.

### 6. JS and CLI agreement/validation

Extend the P8 agreement fixtures rather than adding a second harness:

- Add an SDK/CLI case whose request carries an opaque offset string and whose
  reply carries `nextOffset`. Assert the exact wire field in both the JS SDK
  and spawned CLI paths.
- Add `{offset: 7}`, `{offset: true}`, and `{offset: {}}` to the P8 validation
  group; assert the exact new message and zero binding calls.
- Keep existing no-cursor P8 agreement cases, now expecting `nextOffset: null`.
- Update the base agreement cases too: their terminal read expects
  `nextOffset: null`, and their first-window read with `nextLine: 2` must
  expect a deterministic non-null stand-in cursor.
- Update the exact `--text fs read` CLI assertion to include that deterministic
  `nextOffset` line after `nextLine`.
- Exercise `fs read ... --offset <cursor> --start-line <n>` in an agreement
  case so CLI forwarding and the combined surface are covered. Rust real-Store
  tests own actual precedence behavior; the stand-in owns forwarding only.

**Old-code failures:** old JS rejects `offset` as an unknown option, old CLI
rejects `--offset`, old fixtures/replies omit `nextOffset`, and old validation
uses the unknown-option message instead of the required offset-type message.

### 7. Golden and neighboring regression checks

Update `sdk_method_golden` expectations and run it under `umask 022`; old code
fails the updated golden because `nextOffset` is absent. Re-run existing
`sdk_text_scan`, `area_p8_reads`, `area_p1_history`, `area_p14_paging`, and
`area_p7_attribution` to ensure scanner/shared-cursor neighbors remain exact.
Run JS `area-p`, `calls`, and `cli.test` for surface compatibility.

These neighboring suites are regression evidence, not new discriminating
finding-7 tests; no claim is made that every unchanged neighbor fails on old
code.

## Rejected alternative: immutable-content line-index cache

A bounded cache keyed by immutable `content_root` could map line numbers to
byte offsets and avoid the public API addition. It remains inferior for this
finding:

1. A cold cache still needs a full content scan before a late numeric line
   lookup. The cursor avoids that scan for the supported sequential workflow
   from the second page onward.
2. Correct hits require exact content identity and consistent line/offset
   entries. Eviction, partial-index representation, duplicate-content sharing,
   memory bounds, concurrent construction, and cold/warm behavior all become
   product semantics that need separate proofs.
3. Eviction or a cold process can silently reintroduce quadratic sequential
   behavior. A caller-held cursor makes resumption explicit and deterministic
   across processes and cache states.
4. The existing per-call cache lifetime is intentional. Extending it into a
   retained line index would be a larger ownership/resource change than
   carrying a small continuation string through the existing request/reply.
5. The cursor adds no Store writes, index maintenance, background jobs, or
   invalidation path. It also works naturally across immutable snapshots that
   share content.

The cache could still accelerate random numeric line-only access, but that is
a different requirement and does not remove the quadratic cold sequential
case. The design work therefore does not overturn the retro conclusion: the
byte cursor is the simpler durable shape for sequential reads. Numeric-only
calls intentionally retain legacy performance and semantics.

## Implementation touch list

No change is planned in `read/cached.rs`, `layerfs-node`, Store schemas,
history cursors, transport framing, Python, or MCP.

### Production and contract documentation

- `engine/crates/layerfs-store-service/src/read/window.rs`
  - Track absolute line-start offsets; accept a byte/line start; range-read
    from that offset; preserve absolute classification windows and bounded
    unreturned-line rescanning.
- `engine/crates/layerfs-store-service/src/read/reads.rs`
  - Parse/validate the typed cursor against path/content/mode/size; implement
    precedence and effective start; construct `nextOffset` beside `nextLine`.
- `engine/crates/layerfs-api/src/fs/read.rs`
  - Add optional `offset` argument parsing and pass it to the service.
- `api/agent/src/fs/read.js`
  - Allow/validate `options.offset` and forward it.
- `api/cli/src/fs/read.js`
  - Add `--offset` and forward it through the SDK options object.
- `api/docs/agent.md`
  - Document JS/CLI shapes, continuation use, binding, precedence, mode rules,
    and refusals.
- `docs/general/sdk.md`
  - Document the additive read contract and complexity behavior without
    claiming unverified measurements.

### Tests and fixtures

- `engine/crates/layerfs-api/tests/area_p8_reads.rs`
  - Add two-size counter traversal, exactly-once/boundary, precedence,
    compatibility, format/binding refusal, and cross-snapshot cases.
- `engine/crates/layerfs-api/tests/sdk_text_scan.rs`
  - Add cursor/reference equivalence and delayed-Binary/mode-preservation
    cases.
- `engine/crates/layerfs-api/tests/sdk_method_golden.rs`
  - Add/retain golden calls needed for the additive reply and cursor argument
    surface; no performance evidence comes from this golden.
- `engine/crates/layerfs-api/tests/golden/sdk_methods.txt`
  - Update successful `fs.read` replies with `nextOffset` and any added golden
    refusal/continuation rows.
- `api/agent/test/fixture/cases-area-p.mjs`
  - Add SDK/CLI offset agreement cases and `nextOffset` result expectations.
- `api/agent/test/fixture/cases.mjs`
  - Update the base read replies: null on the terminal page and a deterministic
    non-null stand-in cursor on the existing first-window continuation.
- `api/agent/test/fixture/fake-native.cjs`
  - Return the additive field and provide deterministic cursor/refusal
    stand-in behavior needed by agreement cases.
- `api/agent/test/area-p.mjs`
  - Extend the existing P8 options-validation group for offset types and exact
    no-binding-call behavior.
- `api/cli/test/cli.test.mjs`
  - Update the exact `--text fs read` output assertion with `nextOffset`; the
    shared agreement cases and generated documentation tables cover the other
    CLI assertions without a second harness.

## Open questions for the commander

None. The wire field names, cursor representation, binding, refusal messages,
precedence, classification behavior, JS/CLI surfaces, and test acceptance
shapes are fully specified above. The only launch precondition outside this
design is independent confirmation of the `6c9111ec1` base in the private git
checkout by the commander, because the supplied workdir has no git metadata.
Implementation must wait for commander approval and a separate implementation
brief.
