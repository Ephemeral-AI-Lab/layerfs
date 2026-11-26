# Doctor Findings Catalogue & Fault-Injection Receipts

> **Status:** Current architecture guide for LayerFS 2.0.0 (`layerfs_v2_0_0`); maintained with `core/docs/issues/call-surface-2-0/calls.json` and `control/crates/layerfs-control/src/doctor/`.

This catalogue lists every check ID emitted by the six LayerFS 2.0 doctors (`home.doctor`, `machine.doctor`, `store.doctor`, `project.doctor`, `sandbox.doctor`, `backup.doctor`) and the `workspace` child nodes, together with failure causes, `evidence` keys (`--detail`), the `fix` the code emits, safety classifications (`read`, `safe`, `disruptive`, `destructive`), and live fault-injection verification receipts.

The doctor never runs a fix. A `fix` is one runnable shell line (two commands are joined with `&&`). "No fix" means the code emits none: no single call repairs the cause, or the call would lose data, so the `message` says what to do by hand. A `<name>` in a fix is a placeholder the doctor cannot know (a bucket's URL, a file to restore). Evidence holds paths, ids, state words and error code words; never a secret, a URL with a password, or debug text.

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
| `home.prerequisites` | Host lacks root, `/dev/fuse` or cgroup2 for a Sandbox | `missing` | No fix: host setup, not a `layerfs` call | `read` |
| `home.layout` | Home folder is a foreign/pre-2.0 layout, or the system refused to read it | `path`, `layout` (foreign); `path`, `code` (unreadable) | Foreign: `mv <home> <home>.old && layerfs home init`. Unreadable: no fix, the OS error is the finding | `disruptive` |
| `home.records` | See the table below | See below | See below | `safe` / `destructive` |
| `home.bundle` | Linux bundle manifest missing, malformed or of another build; or it lacks a program or image | `path`, `platform`, `expected_version`, `found_version`, `manifest`; or `path`, `missing` | No fix: no `layerfs` call installs a bundle, it ships with the install | `read` |
| `home.native_daemon` | `layerfs-daemon` not executable, missing, or from another commit (missing is an error when recorded, a warning for the default) | `path`, `source`, `commit`, `dirty`, `matches` | Not executable: `chmod +x <daemon>`. Missing or another commit: `layerfs machine update native --daemon <path>`, only when this build's daemon is found; otherwise no fix | `safe` |
| `home.artifacts` | A machine's configured daemon file is not executable, or is not there | `machine`, `path`, `executable` (not executable) or `present` (missing) | Not executable: `chmod +x <file>`. Missing: no fix (the daemon of another machine is not one this host knows); the message names `layerfs machine update <m> --daemon <path>` | `safe` |
| `home.capacity` | Free disk space under the home is critically low or cannot be measured | `filesystem`, `path`, `disk_free_bytes` | No fix: which Store to compact is a choice, and compaction needs room | `read` |

`home.records` causes, each with the fix the code emits:

| Cause | `evidence` Keys | `fix` |
| --- | --- | --- |
| Store record cannot be read | `store`, `path`, `code` | No fix: `store remove` would delete the Store and its data; repair the file by hand |
| Store on a machine with no record | `store`, `machine`, `machine_record` | No fix: both remedies are by hand |
| Stale Store lease | `store`, `pid`, `path` | Cleanup occurs under the Store lock when an operation claims or checks leases |
| Host-worker directory has unsafe ownership, type or modes | `path`, `code` | Inspect the directory named in the finding |
| Built-in machine's recorded daemon not there | `machine`, `daemon`, `present` | `layerfs machine update native --daemon <path>` for `native` when this build's daemon is found; else no fix |
| Unresolved `op` on a Store, machine or Sandbox | `op`, `phase`, `op_id`, `standing` | The clearing call of that thing (for a Sandbox, `layerfs sandbox remove <x>`) |
| Sandbox names no project, Store, project or machine with a record | `sandbox`, `reference`, `expected` | `layerfs sandbox remove <x>` |
| Copy a Store move left on another machine | `store`, `call`, `from`, `to`, `on` | No fix: only the next `store move` of that Store deletes it |
| Partial `store.sqlite.incoming` copy | `store`, `path` | `rm <file>` |
| File mode wrong (`0644`, `0755`) | default (`kind`, `name`) | `chmod 600 <record>`, `chmod 700 <dir>` or `chmod g-rwx,o-rw <path>` |
| Leftover Workspace folder | default | `rm -r <folder>` |
| Stale exec record | default | `layerfs workspace cancel <workspace> --exec <id>` |
| Symlink, integrity or other by-hand repair | default | No fix: no command repairs it |

## 2. `machine.doctor` (`machine.*`, 11 checks; `native` runs 3, 4 on Linux as root)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `machine.record` | No record (not built-in); record unreadable; record not valid (version, name, state, op, sizes); platform removed; platform unknown or missing | `record`, `built_in` (no record); `code`, `path` (unreadable); `problem`, `path` (invalid); `platform`, `code` (removed); `code` (unknown) | `layerfs machine remove <m>` only when it removes records alone: a record that parses but is not valid and names no VM (no `vm_name`), or a removed platform, and the machine is not built in (nothing on the host is touched). A record that is not valid and names a VM gets no fix, because `machine remove` deletes the VM and its disk; a built-in machine gets none, because remove refuses it. For a record that is not valid JSON, or names an unknown platform, `machine remove` fails too, and the doctor gives no fix: repair or delete the file by hand. No record: no fix (`machine create` needs a platform) | `safe` |
| `machine.operation` | Unresolved `op` left by an ended owner | `standing`, `uncertain`, `id`, `name`, `phase` | `layerfs machine remove <m>` (after a create/remove) or `layerfs machine get <m> --with live` (after a start/stop) | `safe` |
| `machine.configuration` | Record cannot form a valid provider request; or this build has no machine adapters (omitted on `native`) | `code`, `nameless`; or `adapters` | The detail's own fix when it has one; otherwise none | `disruptive` |
| `machine.provider` | Provider unreachable or VM/container not running (omitted on `native`) | `state`, `runtime`, `tool_version`; `code`, `runtime` for a failed call | Engine down: start Docker. VM missing: `layerfs machine remove <m>` (after removing what is on it). Provider says failed: `layerfs machine start <m>`. Platform unsupported on this host, or a failed provider call: no fix | `safe` |
| `machine.state` | Recorded state disagrees with live provider state | `recorded`, `observed` | `layerfs machine get <m> --with live` | `safe` |
| `machine.worker.endpoint` | Saved worker endpoint/secret missing or unreadable | `saved` | `layerfs machine start <m>` | `safe` |
| `machine.worker.auth` | Worker does not answer, or rejects the saved secret | `code` | `layerfs machine stop <m> && layerfs machine start <m>` (the stop gets `--force` when Sandboxes are recorded on the machine) | `disruptive` |
| `machine.worker.compatibility` | Worker build does not match calling `layerfs` build | `hello` | `layerfs machine stop <m> [--force] && layerfs machine start <m>` (`--force` when Sandboxes are recorded) | `disruptive` |
| `machine.worker.ping` | Worker's answer names another process or build; or it refused a request | `kind`, `version` (another build); `code` (refused) | Another build: `layerfs machine stop <m> [--force] && layerfs machine start <m>`. Refused: `layerfs machine start <m>` | `disruptive` / `safe` |
| `machine.capacity` | Machine data disk is low on free space | `reclaimable_image_bytes`, `disk_free_bytes` | `layerfs machine prune <m> --unused`, only when reclaimable bytes are above 0; else no fix | `safe` |
| `machine.network` | IPv4 forwarding off; `table inet layerfs` missing while Sandboxes are recorded; `lfs*` links no Sandbox recorded in the home owns, on any machine (a warning: on a Linux host they may be another home's; never when a Sandbox record cannot be read, which leaves the check `not_checked`) | `table`, `forwarding`, `links[{link, addresses, sandbox}]` | stop and start the machine (a Linux host: `sysctl -w net.ipv4.ip_forward=1`; a Sandbox start writes the table); for links, `ip link delete <link>` on the machine once nothing holds them | `disruptive` |

The `native` host worker's endpoint, auth, compatibility and ping checks
describe its Unix socket, verified peer UID, home and build. A different
build yields a compatibility notice with `build` and `expected_build`.
Wait for idle exit. A successful ping carries ready or draining evidence,
including the reported process identity and counts when available.

## 3. `store.doctor` (`store.*`, 6 checks)

A backup can be restored under a new Store name, but identity, schema and data problems get no in-place restore fix: `backup restore` refuses to replace a recorded Store whose current heads cannot be proven unchanged, and replacing the whole Store can discard newer data. They never get a remove.

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `store.record` | Unreadable record, name or version not this Store's, saved id invalid, or unsupported engine | `code`; `name_matches`, `version_matches`; `reason` | Unreadable record: no fix, unless the refusal carries its own (`store remove` would delete the Store and its projects). Unsupported engine: `layerfs store remove <s>`. Name or version not this Store's, or saved id invalid: no fix, the record may be another build's or another Store's | `safe` |
| `store.operation` | Unresolved `op` in the home record, unresolved compaction, or observed lease holders (notice) | `operation`, `standing`; `operation`, `phase`; `stale_pids`; `holders`, `holder_count` | Op: the clearing call. Compaction: `layerfs store compact <s>`. Stale lease cleanup occurs under the Store lock when an operation claims or checks leases | `safe` |
| `store.connection` | SQLite file missing; PG auth, transport/TLS, or 5 s silent handshake failure; machine did not answer | `reason`, `machine` (file); `step` (`login`, `transport`, `handshake`, `authentication`), `host`; `machine`, `code` | File missing with a bucket link: no fix, because no single call repairs it in place; check the machine's mounts first if the file may come back, or restore the backup under a new Store name. When the Store has no bucket link: `layerfs store remove <s>`, which deletes its record and every project record in it (the message says so; check the machine's mounts first if the file may come back). PG authentication or login: `layerfs store connect <s> <url> --password-stdin` (the `password_fix` line; the refusal's own fix, none when the record names no user); other PG steps: `layerfs store connect <s> <url>`. Machine did not answer: no fix; the message names `layerfs machine doctor <machine>` as the diagnostic next step. A busy machine, or a host that cannot open sqlite: no fix | `destructive` for the remove, else `safe` |
| `store.identity` | Database/file binding ID does not match saved record `binding` | `reason`, `machine` (PG: `step`) | SQLite: no fix; replacing the file in place can destroy the Store whose id the file really holds. Restore a backup under a new Store name or move the right file back by hand. PostgreSQL: no fix (backup refuses a PostgreSQL Store and restore makes only sqlite); the message says to reconnect to the intended database or restore it with PostgreSQL's own tools | `read` |
| `store.schema` | Schema version or layout incompatible with this build; an older schema version this release does not migrate (#776) carries `reason: outdated` | `reason` (`outdated`), `machine` (PG: `step`) | For `outdated`: `layerfs store remove <s>` (a new Store takes its place; #776). Otherwise: no fix. The message says to use the LayerFS build that wrote it or to restore a backup under a new Store name; replacing the file in place can discard newer data. PostgreSQL gets no fix, and the message adds "restore it with PostgreSQL's own tools" | `destructive` for the `outdated` remove, else `read` |
| `store.backups` | SQLite Store heads are ahead of recorded bucket link heads (notice per bucket) | `bucket` | `layerfs backup create <s> <bucket>` | `safe` |

## 4. `project.doctor` (`project.*`, 3 checks)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `project.record` | Record has no complete manifest; home has no record; record unreadable or parent Store missing | `manifest`; `recorded`; `code` | The refusal's own fix when it carries one; otherwise no fix: restoring from a backup replaces the Store's data, so the owner decides, and `project delete` would drop a record whose data may be intact | `read` |
| `project.listed` | Project record exists in home, but Store catalog does not list it (`notice`) | `recorded`, `listed` | `layerfs project delete <store>/<project>` (removes the record only) | `destructive` |
| `project.data` (`--full`) | Missing or corrupted object/pack reachable from `main` or any Branch head (the first 64 are listed, then a count) | `outcome`, `more` | No fix: replacing the whole Store can discard newer data, PostgreSQL Stores are not restorable by `backup restore`, and an unknown outcome does not establish corruption; never delete the Store | `read` |

## 5. `sandbox.doctor` (`sandbox.*`, 18 checks; `native` runs 15) & `workspace.*` (2 checks)

Where the fix is `layerfs sandbox remove <x>`, that is the plain call: it refuses a Sandbox that is in use or busy instead of discarding it. No row uses `--force` on `sandbox remove`.

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `sandbox.record` | Sandbox name not valid, no record, or record unreadable / unsupported version | `cause`, `v`, `expected_v` | No fix: `remove` refuses an invalid name, answers `NoSuchSandbox` for no record, and cannot act on a record it cannot read. For an unreadable or newer record, use the build that wrote it | `read` |
| `sandbox.machine_reference` | Referenced machine record does not exist | `machine`, `cause` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.store_reference` | Referenced project's Store record unreadable or missing | `store`, `cause` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.project_reference` | Referenced project record missing or malformed | `project`, `cause` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.store_placement` | SQLite Store and Sandbox are recorded on different machines | `store`, `store_machine`, `sandbox_machine` | `layerfs store move <store> <machine>` (moves the Store; nothing is deleted) | `disruptive` |
| `sandbox.operation` | Stale `op` of an ended owner (holds nothing), or an unresolved `op` | `op`, `phase`, `standing` | Stale: no fix, the next call clears it. Unresolved: `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.runtime` | Runtime process ended (pid/start/boot changed), or the record has no/stopped runtime; or the record's identity is incomplete or names no daemon (a warning) | `pid`, `runtime`, `observed`, `workspaces_recorded`; `runtime`; `pid_present`, `start_present`, `boot_present`; `daemon` | Ended or stopped runtime: `layerfs sandbox remove <x>`. The message names how many recorded Workspaces' uncommitted changes are lost. Incomplete identity or no daemon: no fix, the doctor only observes | `destructive` |
| `sandbox.network` | A running namespaces Sandbox's init holds no network namespace of its own, or its host end `lfs<id12>` is missing or lacks its recorded address (omitted for a native Sandbox, and when the route is forwarded) | `link`, `network` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.route` | Saved forward names no address or another machine; or the machine's worker record is malformed or refused the saved secret | `cause`, `forward_machine`, `sandbox_machine`, `address_present`; `machine`, `cause`, `sandboxes_on` | Forward: `layerfs sandbox remove <x>`. Worker: `layerfs machine stop <m> --force && layerfs machine start <m>`; the message and evidence name how many Sandboxes the forced stop affects | `destructive` / `disruptive` |
| `sandbox.forward_process` | Forward process gone (omitted on `native`), or its identity incomplete | `pid`, `observed`; `pid_present`, `start_present`, `boot_present` | Gone: `layerfs sandbox remove <x>`. Incomplete identity: no fix | `destructive` |
| `sandbox.worker_compatibility` | Machine worker build mismatch (omitted on `native`) | `machine`, `worker`, `expected`, `sandboxes_on` | `layerfs machine stop <m> [--force] && layerfs machine start <m>` (`--force` when Sandboxes are recorded) | `disruptive` |
| `sandbox.daemon_keys` | Saved daemon endpoint/identities missing, unreadable, malformed or not the forward's | `cause`; `endpoint_ok`, `secret_ok`, `peer_ok`, `instance_ok`; `forward_address`, `saved_address` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.daemon_connect` | Daemon refuses or times out connection | `cause` | `layerfs sandbox remove <x>` (check `sandbox.runtime` first) | `destructive` |
| `sandbox.daemon_auth` | Daemon rejects saved identities | `cause` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.daemon_instance` | Running daemon instance ID does not match Sandbox record | `cause` | `layerfs sandbox remove <x>` | `destructive` |
| `sandbox.daemon_ready` | Daemon startup failed and was retained, or it is still starting | `phase` | Failed: `layerfs sandbox remove <x>`. Still starting: no fix; re-running the doctor only observes startup | `destructive` / `read` |
| `sandbox.session` | Doctor diagnostic session did not close cleanly | — | Re-run `layerfs sandbox doctor <x>` | `read` |
| `sandbox.observation` | Daemon record changed during diagnostic read | — | Re-run `layerfs sandbox doctor <x>` | `read` |
| `workspace.mounted` | Daemon reports Workspace mount absent (recorded but not held, or held with no mount); a mount being made or removed is a notice with no fix | `recorded`, `listed`; `id`, `activity`, `mounted` | `layerfs workspace unmount <ws> --discard --force` | `destructive` |
| `workspace.activity` | Failed or uncertain operation left on Workspace | `id`, `activity`, `mounted` | `layerfs workspace unmount <ws> --discard --force` | `destructive` |

## 6. `backup.doctor` (`bucket.*`, 9 checks; 5 are `--full` only)

| Check ID | Failure / Warning Cause | `evidence` Keys (`--detail`) | Remedy (`fix` / Next Step) | Safety |
| --- | --- | --- | --- | --- |
| `bucket.configuration` | Bucket record absent, unreadable or invalid; identity/version invalid; provider unsupported | `cause`, `provider` | Absent record or invalid record version: `layerfs backup connect <name> <url> --access-key <key> --key-stdin` (the `<url>` is the bucket's own when its record reads, else a placeholder; connecting again keeps the Stores' links). Unreadable record, wrong saved name, unsupported provider, or unvalidated endpoint/bucket/prefix/region/key: no fix, because reconnecting the same record cannot repair that cause and disconnecting would drop Store links | `safe` |
| `bucket.credentials` | Saved secret key missing or unreadable | `cause` | `layerfs backup connect <name> <url> --access-key <key> --key-stdin` | `safe` |
| `bucket.transport` | Bucket endpoint unreachable or TLS failure | `http_status` | Verify endpoint URL, network reachability, and TLS roots | `read` |
| `bucket.list` | Prefix listing refused with saved credentials | `cause`, `http_status`, `phase`, `s3_code` | HTTP 401 or 403: `layerfs backup connect <name> <url> --access-key <key> --key-stdin`. Any other answer: no fix | `safe` |
| `bucket.write_capability` (`--full`) | Bucket versioning is disabled (cannot safely delete test object version) | — | Enable bucket object versioning on the storage provider | `disruptive` |
| `bucket.put` / `bucket.get` / `bucket.delete` / `bucket.reconcile` (`--full`) | Write/read/delete/reconcile round-trip failure or retained test object | `http_status`, `s3_code`, `possible_retained_key`, `owned_version`; `cause` (probe token not generated) | No fix: the cause is the provider's or its permissions; verify write/delete IAM permissions and manually remove `possible_retained_key` if retained | `disruptive` |

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
