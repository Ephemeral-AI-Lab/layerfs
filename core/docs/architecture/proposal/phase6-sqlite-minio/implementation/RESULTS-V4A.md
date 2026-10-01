# V4a native connection diagnostic and treatment

> Status: Research; informative and not a product contract.

Full scope remains in [CHECKLIST](CHECKLIST.md). Prospective declaration:
[V4A-SPEC](V4A-SPEC.md), published at `46c3dc7cf0f206731ff63a23a1207f093e60db6b`.

## Count-driven baseline diagnostic

Source `198c25296c7e447348422f98a1a11bf0d3732ccc`. One explicitly declared
instrumented diagnostic of the V3 create/overwrite workload; request/packing/
construction/publication algorithms unchanged. This is cause evidence, not a
second plain speed sample. [Identities](evidence/v4a-connection-diagnostic/identity.json),
[receipt](evidence/v4a-connection-diagnostic/receipt.json),
[actual cumulative observations](evidence/v4a-connection-diagnostic/transport-statistics.json),
[cause arithmetic](evidence/v4a-connection-diagnostic/cause.json),
[raw log](evidence/v4a-connection-diagnostic/daemon.stderr),
[complete invocation](evidence/v4a-connection-diagnostic/invocation.json) and
[owned provider drain](evidence/v4a-connection-diagnostic/provider-drain.json).

The overwrite Commit took 331.744250 ms. Between its predecessor's completed
statistics and its own completion, it opened 37 native metadata connections,
spending 213.298414 ms in connect/authentication: 5.764822 ms per connection,
64.296% of the observed Commit wall. It issued 29 lookups, 7 registrations and
one publication. Native request/reply work excluding connection setup summed
44.602623 ms. Connection work includes server acceptance and authentication;
this does not isolate cryptography from the server's 5 ms accept polling delay.
The source's repeated accept delay is consistent with the actual per-connection
cost. Persistent sessions are the single next treatment, preserving validation.

The first cumulative snapshot includes one bootstrap plus its first Commit,
38 connections and 218.887334 ms of connection work; it is not a pure Commit
phase number. The complete child was 7.410180 s; proof 19.996917 ms. Both versions'
literal bytes/mode/parent/head and owner cleanup PASS. Cache state is unknown;
all numerical observations remain INELIGIBLE for speed admission. No broader
family, locality, canonical-reference or physical resource gate is passed.

## Treatment status

Persistent-session treatment is NOT_RUN at this report checkpoint. Next: implement
P6META4 exact increasing IDs and bounded session reuse/known idle rotation,
quarantine failed/uncertain sessions without resend, then one sealed full-path
invocation. The original diagnostic and V3 evidence remain immutable.

## Single persistent-session treatment

Source `776f0f879e52e401d03f4a69301d8c076f1b2a59`; one sealed full-provider
treatment invocation. [Identities](evidence/v4a-persistent-session/identity.json),
[receipt](evidence/v4a-persistent-session/receipt.json),
[actual observations](evidence/v4a-persistent-session/transport-statistics.json),
[treatment arithmetic](evidence/v4a-persistent-session/treatment.json),
[raw log](evidence/v4a-persistent-session/daemon.stderr),
[complete child](evidence/v4a-persistent-session/invocation.json) and
[provider drain](evidence/v4a-persistent-session/provider-drain.json).

Both Commits together use one metadata connection (6.111167 ms of initial
connect/authentication, including bootstrap). The overwrite opens zero new
connections and issues the same 29 lookups, 7 registrations and one publication;
its request/reply time totals 28.218874 ms. Observed create Commit82.707000 ms,
overwrite Commit67.960125 ms; Exec14.285958/6.503833 ms. SDK lifecycle5.991987 s,
full collector child9.209285 s, separate proof19.785250 ms. All are actual recorded
values; complete-child and SDK timer boundaries are distinct. No unchanged-arm
repeat, packing/validation omission, extra producer or deadline change.

Literal bytes/mode, retained versions, parent/head and owner cleanup PASS. Four
external real TCP/Noise session tests PASS (reuse, wrong-ID quarantine/no resend,
capacity-before-connect, known idle rotation); these fixtures do not prove
application/MinIO behavior. Host locked Clippy/fmt and locked Darwin/Linux ARMv8
release builds PASS; unchanged engine tests reused. All raw check text is retained
in JSON with its original SHA256 and exact UTF-8 content.

The session-count effect is proved (75 total per-call connections in the
diagnostic, one in the treatment). The numeric timing observations remain cache
INELIGIBLE and are not a qualified speedup comparison. V4a's declared transport
gate is complete; all larger owner groups/locality/resource/canonical gates remain
incomplete. Next: V4b bounded multi-object packs and locator/read windows, followed
by V4c immediate-base edits, indexed changes and incremental namespace publication.
