/**
 * The closed check lists of the LayerFS 2.0 doctors: for each kind, every
 * check its doctor can make, in order, with what the check is (LayerFS's own
 * words, shown as they are) and whether only a Full check makes it. A copy of
 * the frozen call surface's `doctor.checks` (calls.json v23, 2026-11-19);
 * `test/doctor-model.test.mjs` fails when it differs from the copied contract.
 *
 * The page never decides from this table which checks a thing has: it draws
 * the checks a report's node carries, in the node's order. The table gives a
 * check's tip, the width kept for a row's marks, and the list a detail shows
 * before any doctor has run. A check the table does not know still draws.
 */
export interface CheckEntry { check: string, what: string, full_only: boolean }

export const CHECKS: Readonly<Record<string, readonly CheckEntry[]>> = {
  home: [
    { check: 'home.build', what: 'the calling program exists and can run', full_only: false },
    { check: 'home.prerequisites', what: 'what a Sandbox on this computer needs is installed', full_only: false },
    { check: 'home.layout', what: 'the home folder is a current-layout home this build reads', full_only: false },
    { check: 'home.records', what: 'every record in the home: references between records, file modes, leases and half-finished operations', full_only: false },
    { check: 'home.bundle', what: 'the Linux bundle is present and complete, where a recorded machine needs it', full_only: false },
    { check: 'home.native_daemon', what: 'the daemon for this computer is present and from this build', full_only: false },
    { check: 'home.artifacts', what: 'each machine\'s configured daemon file is present and can run', full_only: false },
    { check: 'home.capacity', what: 'free space on the disk that holds the home can be measured', full_only: false },
  ],
  machine: [
    { check: 'machine.record', what: 'its record is valid and names a platform this build runs', full_only: false },
    { check: 'machine.operation', what: 'no operation on it is half-finished or left with its outcome not known', full_only: false },
    { check: 'machine.configuration', what: 'its record makes a valid request to its provider', full_only: false },
    { check: 'machine.provider', what: 'its provider answers and its VM or container runs', full_only: false },
    { check: 'machine.state', what: 'the recorded state agrees with what the provider reports', full_only: false },
    { check: 'machine.worker.endpoint', what: 'a worker endpoint and secret are saved for it', full_only: false },
    { check: 'machine.worker.auth', what: 'the worker accepts the secret and answers hello', full_only: false },
    { check: 'machine.worker.compatibility', what: 'the worker is from this build', full_only: false },
    { check: 'machine.worker.ping', what: 'the worker answers a request', full_only: false },
    { check: 'machine.capacity', what: 'its data disk has room', full_only: false },
  ],
  store: [
    { check: 'store.record', what: 'its record is valid: name, engine and the saved id', full_only: false },
    { check: 'store.operation', what: "no operation on it is half-finished or left with its outcome not known, in the home's record or, for a sqlite Store, beside its file on its machine; and no lease is stale", full_only: false },
    { check: 'store.connection', what: 'it is reached where it lives: a sqlite Store through its machine; a PostgreSQL Store from this computer, and once more from each machine that has a Sandbox of it, with `from` naming that machine', full_only: false },
    { check: 'store.identity', what: 'the id in its data is the id in its record', full_only: false },
    { check: 'store.schema', what: 'its schema is the one this build reads', full_only: false },
    { check: 'store.mount_holders', what: 'durable mount holders have matching recorded workspaces', full_only: false },
    { check: 'store.backups', what: 'no backup of it is behind: its heads against each bucket link (sqlite; a notice per bucket)', full_only: false },
  ],
  project: [
    { check: 'project.record', what: 'the home\'s record of the project and its folder read, and the Store the record sits under is on record', full_only: false },
    { check: 'project.listed', what: 'the Store\'s own list holds the project the record names, by its stack id, or by its name for a record without one', full_only: false },
    { check: 'project.mount_holders', what: 'durable mount holders have matching recorded workspaces', full_only: true },
    { check: 'project.data', what: 'everything reachable from main\'s head and from each Branch\'s head is read and checked', full_only: true },
  ],
  sandbox: [
    { check: 'sandbox.record', what: 'its record exists, parses and has a supported version', full_only: false },
    { check: 'sandbox.machine_reference', what: 'the machine its record names has a record', full_only: false },
    { check: 'sandbox.store_reference', what: 'the Store of its project has a readable record', full_only: false },
    { check: 'sandbox.project_reference', what: 'its project has a record', full_only: false },
    { check: 'sandbox.store_placement', what: 'a sqlite Store and the Sandbox are on the same machine', full_only: false },
    { check: 'sandbox.operation', what: 'no operation of an ended owner is left on its record', full_only: false },
    { check: 'sandbox.runtime', what: 'its recorded runtime process runs, by id, start time and boot', full_only: false },
    { check: 'sandbox.route', what: 'the saved route to its daemon can be used: its machine\'s worker, or its forward', full_only: false },
    { check: 'sandbox.forward_process', what: 'the forward\'s process runs, where the route has one', full_only: false },
    { check: 'sandbox.worker_compatibility', what: 'its machine\'s worker is from this build, where the worker makes the report', full_only: false },
    { check: 'sandbox.daemon_keys', what: 'the saved daemon endpoint and identities are readable and match the route', full_only: false },
    { check: 'sandbox.daemon_connect', what: 'the daemon accepts a connection', full_only: false },
    { check: 'sandbox.daemon_auth', what: 'the daemon accepts the saved identities', full_only: false },
    { check: 'sandbox.daemon_instance', what: 'the daemon is the instance the Sandbox was started with', full_only: false },
    { check: 'sandbox.daemon_ready', what: 'the daemon reports that it is ready for calls', full_only: false },
    { check: 'sandbox.session', what: 'the doctor\'s own session with the daemon ended cleanly', full_only: false },
    { check: 'sandbox.observation', what: 'the daemon\'s record did not change while it was read', full_only: false },
  ],
  workspace: [
    { check: 'workspace.mounted', what: 'the daemon reports its mount as present', full_only: false },
    { check: 'workspace.activity', what: 'no failed or uncertain operation is kept on it', full_only: false },
  ],
  bucket: [
    { check: 'bucket.configuration', what: 'its record is valid: provider, address, bucket, prefix and key', full_only: false },
    { check: 'bucket.credentials', what: 'its key can be read from the home', full_only: false },
    { check: 'bucket.transport', what: 'its address answers', full_only: false },
    { check: 'bucket.list', what: 'one listing page under its prefix succeeds with its key', full_only: false },
    { check: 'bucket.write_capability', what: 'the bucket keeps object versions, so a test object can be removed again', full_only: true },
    { check: 'bucket.put', what: 'one small test object is written', full_only: true },
    { check: 'bucket.get', what: 'the test object is read back', full_only: true },
    { check: 'bucket.delete', what: 'the test object\'s version is deleted', full_only: true },
    { check: 'bucket.reconcile', what: 'after a put whose reply was lost: the test object is found and owned, without a second put', full_only: true },
  ],
}

const BY_ID = new Map(Object.values(CHECKS).flat().map((c) => [c.check, c]))

/** What a check is, in LayerFS's words; empty for a check the table does not know. */
export const whatOf = (check: string): string => BY_ID.get(check)?.what ?? ''

/** The checks a doctor makes on a kind without a Full check: what a detail lists before any doctor has run. */
export const quickChecks = (kind: string): readonly CheckEntry[] => (CHECKS[kind] ?? []).filter((c) => !c.full_only)
