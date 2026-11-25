# Doctor Findings Catalogue & Fault-Injection Receipts

> **Status:** Current architecture guide for LayerFS 2.0.0 (`layerfs_v2_0_0`); maintained with `core/docs/issues/call-surface-2-0/calls.json` and `control/crates/layerfs-control/src/doctor/`.

This catalogue lists every check ID emitted by the six LayerFS 2.0 doctors (`home.doctor`, `machine.doctor`, `store.doctor`, `project.doctor`, `sandbox.doctor`, `backup.doctor`) and the `workspace` child nodes, together with failure causes, `evidence` keys (`--detail`), recommended next steps, safety classifications (`read`, `safe`, `disruptive`, `destructive`), and live fault-injection verification receipts.

## Safety Levels

| Tag | Meaning |
| --- | --- |
| `read` | Observational only; writes no state. |
| `safe` | Repairs metadata, permissions, or restarts/starts an idle service without discarding user data. |
| `disruptive` | Interrupts a running machine, daemon, or command; may require stopping active Workspaces first. |
| `destructive` | Removes a Store, project record, or Sandbox/Workspace (may discard uncommitted Workspace edits). |

---

## 1. `home.doctor` (`home.*`, 8 checks)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `home.build` | Calling binary missing or unexecutable | `path` | Reinstall or rebuild the `layerfs` CLI binary | `safe` |
| `home.prerequisites` | Host Sandbox prerequisites missing (e.g. non-root Linux `namespaces`/FUSE) | — | Run in a supported Linux root/VM/CI environment or configure a machine | `read` |
| `home.layout` | Home folder is a foreign/pre-2.0 layout or unreadable | — | `mv <home> <home>.old && layerfs home init` | `disruptive` |
| `home.records` | Unreadable/invalid `record.json`, bad permissions (`0644`/`0755`), stale lease, unresolved `op`, or dangling reference | — | `chmod 600 <record>`, `chmod 700 <store-dir>`, `rm <lease>`, `layerfs sandbox remove <x>`, or entity clearing call (`checks.md`) | `safe` / `destructive` |
| `home.bundle` | Linux bundle missing or incomplete when a recorded machine needs it | `path` | Rebuild or reinstall the matching Linux bundle | `safe` |
| `home.native_daemon` | `layerfs-daemon` missing, non-executable, or build mismatch | `path` | `chmod +x <daemon>` or rebuild `layerfs-daemon` from the same commit | `safe` |
| `home.artifacts` | Configured per-machine daemon path missing or not executable | `path` | `layerfs machine update <m> --daemon <path>` | `safe` |
| `home.capacity` | Free disk space on home filesystem cannot be measured or is critically low | `free_bytes`, `total_bytes` | Free disk space or run `layerfs store compact <s>` | `safe` |

## 2. `machine.doctor` (`machine.*`, 11 checks; `native` runs 3, 4 on Linux as root)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `machine.record` | Record unreadable, wrong version, unsupported platform, or missing `vm_name` | — | `layerfs machine remove <m>` (records only; built-ins recreate cleanly) | `safe` |
| `machine.operation` | Unresolved `op` left by an ended owner | — | `layerfs machine remove <m>` (after create/remove) or `layerfs machine get <m> --with live` | `safe` |
| `machine.configuration` | Record cannot form a valid provider request (omitted on `native`) | — | Recreate or update machine configuration | `disruptive` |
| `machine.provider` | Provider unreachable or VM/container not running (omitted on `native`) | — | `layerfs machine start <m>` (or start Docker/Lima provider) | `safe` |
| `machine.state` | Recorded state disagrees with live provider state | — | `layerfs machine get <m> --with live` or `layerfs machine start <m>` | `safe` |
| `machine.worker.endpoint` | Saved worker endpoint/secret missing or unreadable | — | `layerfs machine stop <m> && layerfs machine start <m>` | `disruptive` |
| `machine.worker.auth` | Worker rejects saved secret or fails hello | — | `layerfs machine stop <m> && layerfs machine start <m>` | `disruptive` |
| `machine.worker.compatibility` | Worker build does not match calling `layerfs` build | — | `layerfs machine stop <m> && layerfs machine start <m>` (or `--force` if Sandboxes are running) | `disruptive` |
| `machine.worker.ping` | Worker does not answer ping within budget | — | `layerfs machine stop <m> && layerfs machine start <m>` | `disruptive` |
| `machine.capacity` | Machine data disk is low on free space | `reclaimable_image_bytes`, `free_bytes` | `layerfs machine prune <m> --unused` | `safe` |
| `machine.network` | IPv4 forwarding off; `table inet layerfs` missing while Sandboxes are recorded; `lfs*` links no Sandbox recorded in the home owns, on any machine (a warning: on a Linux host they may be another home's; never when a Sandbox record cannot be read, which leaves the check `not_checked`) | `table`, `forwarding`, `links[{link, addresses, sandbox}]` | stop and start the machine (a Linux host: `sysctl -w net.ipv4.ip_forward=1`; a Sandbox start writes the table); for links, `ip link delete <link>` on the machine once nothing holds them | `disruptive` |

## 3. `store.doctor` (`store.*`, 6 checks)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `store.record` | Unreadable record or unsupported engine | — | `layerfs store remove <s>` for an unusable sqlite record; `layerfs store disconnect <s>` then `layerfs store connect <s> <url> --user <user> --password-stdin` for a PostgreSQL one | `safe` |
| `store.operation` | Unresolved `op` in home record, stale `<store>.compact-op`, or stale lease | — | `layerfs store compact <s>`, `layerfs store move <s> <to>`, `layerfs store remove <s>`, or `layerfs store get <s> --with live` | `safe` |
| `store.connection` | SQLite file missing/unreadable; or PG auth (`28P01`), transport/TLS, or 5 s silent handshake failure (per-machine probes carry `from`) | `step` (`login`, `transport`, `handshake`, `authentication`) | Change the login or URL (`layerfs store connect <s> <url> [--user <user>] --password-stdin`), attach a machine (`layerfs store attach <s> --machine <m>`), or restore PostgreSQL server/TLS trust | `safe` |
| `store.identity` | Database/file binding ID does not match saved record `binding` | — | Reconnect to the intended Store or restore the authoritative Store copy; never delete | `read` |
| `store.schema` | Schema version or layout incompatible with this build; an older schema version this release does not migrate (#776) carries `reason: outdated` | `reason` (`outdated`) | Use the matching LayerFS build or migrate/restore from backup; for `outdated` the fix is `layerfs store remove <s>` (a new Store takes its place) | `read` |
| `store.backups` | SQLite Store heads are ahead of recorded bucket link heads (notice per bucket) | — | `layerfs backup create <s> <bucket>` | `safe` |

## 4. `project.doctor` (`project.*`, 3 checks)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `project.record` | Project record or folder unreadable, or parent Store missing | — | Repair record or `layerfs project delete <store>/<project>` | `destructive` |
| `project.listed` | Project record exists in home, but Store catalog does not list it (`notice`) | — | `layerfs project delete <store>/<project>` | `destructive` |
| `project.data` (`--full`) | Missing or corrupted object/pack reachable from `main` or any Branch head | — | Restore Store from backup (`layerfs backup restore`) or authoritative copy; never delete the Store blindly | `disruptive` |

## 5. `sandbox.doctor` (`sandbox.*`, 18 checks; `native` runs 15) & `workspace.*` (2 checks)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `sandbox.record` | Sandbox record missing, invalid JSON, or unsupported version | — | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.machine_reference` | Referenced machine record does not exist | — | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.store_reference` | Referenced project's Store record unreadable or missing | — | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.project_reference` | Referenced project record missing | — | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.store_placement` | SQLite Store and Sandbox are recorded on different machines | — | `layerfs store move <s> <machine>` or recreate Sandbox on the Store's machine | `disruptive` |
| `sandbox.operation` | Unresolved `op` from ended owner on Sandbox record | — | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.runtime` | Recorded daemon runtime process ended or PID/start/boot changed | — | `layerfs sandbox remove <x> --force` (warns uncommitted Workspace data will be discarded) | `destructive` |
| `sandbox.network` | A running namespaces Sandbox's init holds no network namespace of its own, or its host end `lfs<id12>` is missing or lacks its recorded address (omitted for a native Sandbox, and when the route is forwarded) | `link`, `network` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.route` | Saved daemon route (worker or forward) unusable | — | Start/restart machine or `layerfs sandbox remove <x> --force` | `disruptive` |
| `sandbox.forward_process` | Forward process exited (omitted on `native`) | — | Restart machine or `layerfs sandbox remove <x> --force` | `disruptive` |
| `sandbox.worker_compatibility` | Machine worker build mismatch (omitted on `native`) | — | `layerfs machine stop <m> && layerfs machine start <m>` | `disruptive` |
| `sandbox.daemon_keys` | Saved daemon endpoint/identities unreadable or mismatched | — | `layerfs sandbox remove <x> --force` | `destructive` |
| `sandbox.daemon_connect` | Daemon refuses or times out connection | — | Check `sandbox.runtime`; if dead, `layerfs sandbox remove <x> --force` | `destructive` |
| `sandbox.daemon_auth` | Daemon rejects saved identities | — | `layerfs sandbox remove <x> --force` | `destructive` |
| `sandbox.daemon_instance` | Running daemon instance ID does not match Sandbox record | — | `layerfs sandbox remove <x> --force` | `destructive` |
| `sandbox.daemon_ready` | Daemon reports not ready for calls | — | Retry after startup or `layerfs sandbox remove <x> --force` | `disruptive` |
| `sandbox.session` | Doctor diagnostic session did not close cleanly | — | Re-run `layerfs sandbox doctor <x>` | `read` |
| `sandbox.observation` | Daemon record changed during diagnostic read | — | Re-run `layerfs sandbox doctor <x>` | `read` |
| `workspace.mounted` | Daemon reports Workspace mount absent | — | `layerfs workspace unmount <ws> --discard --force` | `destructive` |
| `workspace.activity` | Failed or uncertain operation left on Workspace | — | `layerfs workspace unmount <ws> --discard --force` | `destructive` |

## 6. `backup.doctor` (`bucket.*`, 9 checks; 5 are `--full` only)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `bucket.configuration` | Bucket record missing required provider/url/bucket/prefix/key fields | — | Re-register bucket with `layerfs backup connect <name> <url> ...` | `safe` |
| `bucket.credentials` | Saved secret key missing or unreadable | — | `layerfs backup connect <name> <url> --access-key <key> --key-stdin` | `safe` |
| `bucket.transport` | Bucket endpoint unreachable or TLS failure | `http_status` | Verify endpoint URL, network reachability, and TLS roots | `read` |
| `bucket.list` | Prefix listing fails with saved credentials | `http_status`, `s3_code` | Rotate credentials via `layerfs backup connect ... --key-stdin` | `safe` |
| `bucket.write_capability` (`--full`) | Bucket versioning is disabled (cannot safely delete test object version) | — | Enable bucket object versioning on the storage provider | `disruptive` |
| `bucket.put` / `bucket.get` / `bucket.delete` / `bucket.reconcile` (`--full`) | Write/read/delete/reconcile round-trip failure or retained test object | `http_status`, `s3_code`, `possible_retained_key`, `owned_version` | Verify write/delete IAM permissions and manually remove `possible_retained_key` if retained | `disruptive` |

---

## Live Fault-Injection Verification Receipts (`layerfs_v2_0_0`, Linux x86_64 ext4 + PostgreSQL 18.6)

Executed on disposable homes (`LAYERFS_TEST_TMPDIR` on `ext4`, PostgreSQL `18.6` on `127.0.0.1:55432`):

| # | Injected Failure | Doctor Command | Observed Finding (`--detail`) | Applied Remedy | Post-Remedy Doctor |
| --- | --- | --- | --- | --- | --- |
| 1 | `store/s1/record.json` mode `0644` | `home doctor --detail` | `home.records` (`error`), `fix: chmod 600 <path>` | `chmod 600 <path>` | `home.records: pass` |
| 2 | Stale lease `store/s1/open/9999999` | `home doctor --detail` | `home.records` (`warning`), `fix: rm <lease>` | `rm <lease>` | `home.records: pass` |
| 3 | Unlisted project record `s1/ghost` | `store doctor s1 --detail` | `project.listed` (`notice`), `fix: layerfs project delete s1/ghost` | `layerfs project delete s1/ghost` | `ok: true` |
| 4 | PG Store record missing `auth` | `store doctor pgs --detail` | `store.record` (`error`), `fix: layerfs store save-auth ... && layerfs store update pgs --auth <auth>` | `layerfs store update pgs --auth pg` | `ok: true` |
| 5 | PG Store wrong password | `store doctor pgs --detail` | `store.connection` (`error`), `evidence: {"step":"authentication"}` | `layerfs store save-auth pg --user layerfs_pg --password-stdin` | `ok: true` |
| 6 | PG Store silent TCP listener | `store doctor pgs --detail` | `store.connection` (`error`, bounded <= 5 s), `evidence: {"step":"handshake"}` / `transport` | Restore valid PG URL | `ok: true` |
| 7 | PG `sslmode=require` untrusted cert | `store connect ... --json` | Refusal `Unavailable` (`no connection to the server was made...`, not `Malformed`) | Trust the CA where the Store is dialled: on Linux (OpenSSL) in the system's trusted roots, or name it in `SSL_CERT_FILE` for calls in that process only (a Sandbox's daemon starts with a cleared environment); on macOS (Security framework, which ignores `SSL_CERT_FILE`) trust it in the login or system keychain | Connected `ok: true` |
| 8 | Deleted row in `layerfs.object_location` | `project doctor pgs/app --full --detail` | `project.data` (`error`, `coverage: "full"`), names missing 64-hex object ID | Restore from backup / authoritative copy | Verified detection |
| 9 | Unreachable/invalid bucket record | `backup doctor b1 --detail` | `bucket.configuration` (`error`) | Reconnect bucket with valid config | Verified detection |

Rows 4 and 5 are receipts of a run before the 2026-11-24 PostgreSQL redesign and keep the calls it ran (`store save-auth`, `store update --auth`), which are gone: a password is now changed with `layerfs store connect <s> <url> --password-stdin`, and a record without a `user` or a saved password has no state of its own.
