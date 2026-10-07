# Host transport retirement and SDK Init

> **Status:** Implemented source after `52e1f2e18`; native install/control proof
> follows separately. This is retirement of the host-mediated transport, not an
> algorithmic simplification or a new performance result.

The SDK's public `initialize` function composes the existing Persistence create,
Project complete-root import, History first-Branch fork and consuming seal.
`InitRequest` supplies the host source/output, explicit profile/layout/policy,
catalog authority, stack/Branch identities, scope seed, destination and deadline.
Acquisition tables belong only to this Init. WAL remains selected throughout.
The returned `SealedProject` contains no session, Reader, Save or data service.

`StoreManifest` lives in Bridge and carries the provider kind, daemon-side Store
locator, profile, catalog authority, initial stack/Branch/root, sealed length and
host SQLite version. The daemon version is absent before installation and must
come from the actual installation acknowledgement. Boundaries constrain only
control metadata and native record windows, never total Store or file size.
Cursor authority is redacted from Debug output.

A failed operation returns `InitFailure` with the exact request, original error,
and any acknowledged Init/fork result. No output is removed, operation repeated
or publication inferred from another read. A caller still owns all retained files
and uncertainty. The SDK does not reopen the sealed Store after success.

Removed product subjects are SDK client/runtime, daemon upstream, Bridge logical
codec/contract/native framing, and the excluded API-core. Their active tests and
examples are retired with them. The immutable-object, pack, history, overlay and
Content algorithms remain. SDK now depends on Project for initialization and
cannot depend on Workspace/daemon; daemon cannot depend on SDK. The provider-free
`store/` guard continues to forbid concrete persistence and transport imports.

Bridge keeps its pinned KK handshake, nonce ownership, authenticated bounded
records, before-effect oversized-record refusal, independent close fence and
exact I/O/crypto quarantine. Removing data fragmentation does not change its
native maximum record size or crypto suite. F5/F13 add control records and the
streamed one-time handoff. There is no remote Save or host data fallback.

All E04 traces, receipts and original verdicts remain under their historical
source pins. Its examples/test subject is removed; future execution of those
selections is NOT_RUN — mechanism removed. The historical evidence parser is
retained. No E04 run occurs. Excluded predecessor crates and root reference are
unchanged. See [F12 proof/LOC record](../issues/307/PRE-S8-F12-20261007.md).
