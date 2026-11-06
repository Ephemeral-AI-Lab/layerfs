# Bridge

[Architecture](../../README.md) › [Engine](../README.md) › [Daemon](README.md) › Bridge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The wire between callers and the daemon.

| | |
| --- | --- |
| Component | [Daemon](README.md) |
| Source | [`engine/crates/layerfs-bridge/src`](../../../../engine/crates/layerfs-bridge/src) |

`layerfs-bridge` holds the authenticated channel, every record a controller
and a daemon exchange, the daemon's private setup record and the Store
install manifest. It carries no file data service.

## Duplex channels

```text
controller (initiator)                         daemon (responder)
  TcpStream connect                              TcpListener accept
  initiate(stream, private, daemon public) ---> accept(stream, private,
                                                       control_peer)
     handshake: Noise_KK, prologue "layerfs/cluster-two/native-channel/v1\0"
     two messages, empty payloads, peer static key checked
  Connection { send: Sender, receive: Receiver, peer: VerifiedPeer }
     each direction: own nonce from 0, own scratch buffers
     both directions share one `failed` flag (quarantine)

one record on the socket:
+----------------+--------------------------------------------+
| length u16 BE  | sealed bytes: AEAD(plaintext) + 16-byte tag |
| 16..65535      | plaintext <= MAX_PLAINTEXT_BYTES = 65519    |
+----------------+--------------------------------------------+
```

| Rule | Source |
| --- | --- |
| Suite `Noise_KK_25519_AESGCM_SHA256` on x86, x86_64 and aarch64; `Noise_KK_25519_ChaChaPoly_BLAKE2s` elsewhere. aarch64 refuses to build without the ARMv8 AEAD flags. No runtime suite fallback. | [`profile.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/profile.rs) |
| Both sides know each other's static public key in advance (KK); a wrong peer fails the handshake. | [`handshake.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/handshake.rs) |
| Transport is stateless: each record uses its direction's next explicit nonce; nonce exhaustion is `NonceExhausted`. | [`channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| A record larger than the window is `RecordLimit` with no send and no nonce used; larger flows are several records. | [`channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| Any failed send or receive quarantines both directions; later calls are `Quarantined`. No retry, reconnect or replay. | [`channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs), [`error.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/error.rs) |
| `TCP_NODELAY` is set on both authenticated sockets. | [`channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs) |
| `wait_peer_end` waits once for the peer's EOF after the final reply, then fences; a trailing byte or read error is reported with its `PeerEndPhase`. | [`channel.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/channel.rs), [`ending.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/ending.rs) |
| `ChannelWork` counts calls, records, bytes, I/O and crypto attempts and buffer capacity per direction. | [`types.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/types.rs) |

## Wire types

Every record starts with a 5-byte magic and fits one channel plaintext.

| Magic | Record | Direction | Source |
| --- | --- | --- | --- |
| `LFSC\x01` | `Call`: control request | controller → daemon | [`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs) |
| `LFSA\x02` | `Answer`: control reply | daemon → controller | [`control_reply.rs`](../../../../engine/crates/layerfs-bridge/src/control_reply.rs) |
| `LFSI\x01` | `StoreManifest`: install request | installer → daemon | [`provision_wire.rs`](../../../../engine/crates/layerfs-bridge/src/provision_wire.rs) |
| `LFSR\x01` | `InstallReply`: `Ready`, `Installed`, `Refused` | daemon → installer | [`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs) |
| `LFSF\x01` | `INSTALL_FINISH` marker after the file bytes | installer → daemon | [`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs) |
| `LFSD\x08` | `DaemonSetup`: private config file, not sent on a channel | file → daemon | [`daemon_setup.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_setup.rs) |

```text
field encodings (wire.rs)
  fixed      raw bytes (ids, keys, u16/u32/u64 big-endian)
  blob       u16 BE length + bytes      text = UTF-8 blob
  record     magic + fields, total <= LIMIT = 65519, no trailing bytes
```

`InitialRecord::decode` picks exactly one decoder for a connection's first
record by its magic: `Control(Call)` or `Install(StoreManifest)`; any other
magic is refused with no fallback. Source: [`wire.rs`](../../../../engine/crates/layerfs-bridge/src/wire.rs),
[`initial_record.rs`](../../../../engine/crates/layerfs-bridge/src/initial_record.rs).

## Records

```text
Call   = "LFSC\x01" | id u64 BE (nonzero) | [12|21, scope[32]] | tag | fields
Answer = "LFSA\x02" | id u64 BE           | reply kind | fields
```

| Tag | `Request` | Tag | `Request` |
| --- | --- | --- | --- |
| 1 | `Mount { workspace, branch }` | 9 | `Attach(token)` |
| 2 | `Commit(CommitControl { token, force, message })` | 10 | `Locate(workspace)` |
| 3 | `Status(token)` | 11 | `ForceUnmount { token, relinquish_unknown }` |
| 4 | `Unmount(UnmountControl { token, discard })` | 12 / 21 | `Observed { scope, request, traced }` wrapper |
| 5 | `Fork` | 13 | `Cleanup(token)` |
| 6 | `History` | 14 | `Branches { stack, page }` |
| 7 | `Hello(HelloRequest)` | 20 | `Sdk(SdkCall { method, args })` |
| 8 | `EndSession` | 15 | `MountNamed { workspace, project, branch }` |

An `Observed` wrapper may not wrap `Hello`, `EndSession` or another
observation. `Reply` variants: `Cleanup`, `SessionEnded`, `Hello`,
`Bound`, `Committed`, `MergeCommitted`, `Sdk`, `SdkRefused`, `Conflict`,
`Status`, `Unmounted`, `Dirty`, `Forked`, `History`, `Branches`, `Refused`,
`Ready`, `Located`, `Retained`, `ForceUnmounted`, `Observed`. `Fork`,
`History` and `Branches` still have records but the daemon refuses them
([Control calls](control-calls.md)).

| Value type | Bound | Source |
| --- | --- | --- |
| `WorkspaceToken` | `WorkspaceId` + daemon-local `namespace`, never redirected | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| `SdkValue` | `Null`, `Bool`, `Int`, `Text`, `Bytes`, `List`, `Map`; depth at most 16; method or code at most 64 bytes | [`control_sdk.rs`](../../../../engine/crates/layerfs-bridge/src/control_sdk.rs) |
| History pages | `HISTORY_WINDOW` 32 per page | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| `ForceUnmounted`, `ForcedFacts`, `TeardownCustody` | See [Forced teardown](forced-teardown.md) | [`control_forced.rs`](../../../../engine/crates/layerfs-bridge/src/control_forced.rs), [`control_mount.rs`](../../../../engine/crates/layerfs-bridge/src/control_mount.rs) |
| Commit and merge values | Hex identities shared with the SDK | [`sdk_values.rs`](../../../../engine/crates/layerfs-bridge/src/sdk_values.rs), [`sdk_merge_values.rs`](../../../../engine/crates/layerfs-bridge/src/sdk_merge_values.rs) |

## Daemon setup

| `DaemonSetup` field | Meaning |
| --- | --- |
| `listen` | TCP listen address |
| `private_key`, `control_peer` | Daemon Noise secret; the one authorized controller's public key |
| `route` | `KernelRoute::Fuse` or `Nfs` |
| `overlay` | Overlay template path; Workspace files go beside it |
| `mounts` | Mount home; each Workspace mount is a new child |
| `command_uid`, `command_gid` | Identity presented as every inode's owner |
| `limits` | `DaemonLimits` below |
| `store` | `DaemonStore::Sqlite { locator, install }` or `Postgres { url, schema, synchronous_commit, authority }` |

| `DaemonLimits` field | Meaning |
| --- | --- |
| `connections` | Ordinary control connections; one more slot is reserved for startup |
| `handshake_ms` | I/O wait during authentication and the first record; cleared afterwards |
| `cache_bytes` | Shared immutable cache allowance |
| `namespaces` | Simultaneously admitted Workspaces |
| `pager_kib` | Overlay pager suggestion per connection |
| `serial_low_water` | Unconsumed inode serials below which a create reserves early |

`Sqlite { install: None }` waits for the one-time install; `Some(manifest)`
opens the installed Store at startup. A PostgreSQL URL lives only in this
private record, never in env, argv or `Debug`. Source:
[`daemon_setup.rs`](../../../../engine/crates/layerfs-bridge/src/daemon_setup.rs),
[`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs),
[`support.rs`](../../../../engine/crates/layerfs-bridge/src/support.rs).

## Provisioning

```text
installer (Project Init side)              daemon (one install per process)
  StoreManifest  "LFSI\x01" ------------->  check: Disposable, locator ==
                                             configured Store path
                                            claim <locator>.installing
                                             (create_new), target absent
                 <------------ InstallReply::Ready
  file bytes, records <= 65519 ---------->  write, count bytes == manifest
  INSTALL_FINISH "LFSF\x01" ------------->  rename temp -> locator
                                            open Store, check initial
                                             Branch/root
                 <------------ Installed(manifest + daemon_sqlite)
                   or Refused(InstallRefusal { phase, kind, ... })
```

| `StoreManifest` field | Meaning |
| --- | --- |
| `provider` | `Sqlite`; `Postgres` has no file and is refused |
| `locator` | Absolute Store path in the daemon's filesystem |
| `profile` | Must be `Disposable`; Durable execution is disabled and the daemon refuses it |
| `binding`, `cursor_key` | Catalog authority, sent only on the authenticated channel |
| `stack`, `branch`, `root` | The Init's LayerStack, first Branch and root |
| `bytes`, `host_sqlite`, `daemon_sqlite` | Sealed file length; SQLite versions on each side |

`InstallPhase` (`Request`, `Claim`, `Transfer`, `Publish`, `Open`,
`Reply`) and `RefusalKind` (`Invalid`, `Exists`, `Busy`, `Failed`,
`Unknown`) describe a refusal; neither is a retry instruction. Source:
[`provision.rs`](../../../../engine/crates/layerfs-bridge/src/provision.rs),
[`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs),
[`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs).

## Related

- [Startup](startup.md): the listener, slots and first record.
- [Control calls](control-calls.md): what each request does.
- [Operator: Installs](operator.md#installs): the daemon side of provisioning.
- [Project Init: Seal and install](../project-init/seal-and-install.md): the installer side.
- [API: Routing and workers](../../api/routing/README.md): the controller side of the channel.
- [Glossary](../../glossary.md).

---

← [Daemon: Forced teardown](forced-teardown.md) · [Up: Daemon](README.md) · [Daemon: Operator](operator.md) →
