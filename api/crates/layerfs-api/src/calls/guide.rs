//! What each family, argument and option means, and one example of each
//! call: the words a front end shows beside the call table (the shell's
//! `layerfs help`; the same words suit MCP and the bindings). The meanings
//! follow the SDK reference (`docs/architecture/api/sdk/`). A test checks
//! that every family, argument and option of the table has its words and
//! every call its example.

/// One line on each family.
const FAMILIES: &[(&str, &str)] = &[
    (
        "home",
        "this computer's LayerFS home: its state, call log and doctor",
    ),
    ("project", "projects: files with history, kept in a Store"),
    (
        "branch",
        "Branches: lines of Commits; main moves only by publish",
    ),
    ("commit", "Commits: what a Branch saved, and what differs"),
    (
        "file",
        "read and change a Branch's files; each change is one Commit",
    ),
    (
        "sandbox",
        "Sandboxes: isolated places that run commands for one project",
    ),
    (
        "workspace",
        "Workspaces: a Branch mounted in a Sandbox, run in and committed",
    ),
    ("machine", "machines: where Stores and Sandboxes live"),
    ("store", "Stores: the databases that hold projects"),
    ("backup", "buckets that keep copies of sqlite Stores"),
];

/// What an argument or option means: `(call, name, words)`, where `call`
/// is `family.call` for the words of one call and `*` for the rest.
const ARGS: &[(&str, &str, &str)] = &[
    // positionals of many calls
    ("*", "project", "the project, written <store>/<project>"),
    (
        "*",
        "branch",
        "a Branch of the project; main is the published one",
    ),
    (
        "*",
        "path",
        "an absolute path in the project, such as /src/app.js",
    ),
    (
        "*",
        "commit",
        "a Commit id, or a prefix of 8 or more characters",
    ),
    (
        "*",
        "machine",
        "a machine: native, docker, or one `machine create` made",
    ),
    ("*", "store", "a Store, by its name in this home"),
    ("*", "sandbox", "a Sandbox, by name"),
    (
        "*",
        "workspace",
        "a Workspace, written <sandbox>:<name> as `workspace mount` answers",
    ),
    ("*", "bucket", "a connected bucket, by name"),
    ("*", "auth", "a saved PostgreSQL login, by name"),
    (
        "*",
        "image",
        "a Sandbox image: a sha256: digest, a Docker image id or a reference",
    ),
    ("*", "to", "the absolute path it goes to"),
    (
        "*",
        "content",
        "the file's content, from stdin or --from <file>",
    ),
    (
        "*",
        "changes",
        "a JSON list of changes, from stdin or --from <file>",
    ),
    ("*", "command", "one bash command line; quote it"),
    (
        "*",
        "destination",
        "an absolute path on this computer: absent, or an empty folder",
    ),
    (
        "*",
        "folder",
        "an absolute path of a folder on this computer",
    ),
    ("*", "platform", "the VM kind: lima or apple-container"),
    ("*", "ref", "a Branch or a Commit: the side compared to"),
    (
        "*",
        "file",
        "a Store file in the bucket, as `backup list` names it",
    ),
    // options of many calls
    (
        "*",
        "at",
        "read as of this Commit: its id, or a prefix of 8 or more characters",
    ),
    ("*", "limit", "the most rows in one page, 1 to 1000"),
    (
        "*",
        "cursor",
        "the next_cursor of the page before, to read the next page",
    ),
    (
        "*",
        "expected",
        "refuse Conflict unless the Branch's head is this Commit",
    ),
    ("*", "message", "the Commit message"),
    (
        "*",
        "on_progress",
        "report progress on stderr while it runs",
    ),
    ("*", "mode", "permission bits in octal, such as 0644"),
    ("*", "depth", "how many levels of a folder a page walks"),
    ("*", "byte_offset", "where a file's window starts"),
    (
        "*",
        "byte_length",
        "the window's length in bytes, at most 1048576",
    ),
    ("*", "exclude", "leave out paths that match this glob"),
    ("*", "cpus", "the VM's processors"),
    ("*", "memory", "the VM's memory, such as 4G"),
    (
        "*",
        "synchronous_commit",
        "off, local, remote_write, on or remote_apply",
    ),
    // the words of one call
    (
        "project.init",
        "project",
        "the new project, written <store>/<project>, in an existing Store",
    ),
    ("project.init", "timeout", "the time limit in milliseconds"),
    (
        "project.prune",
        "project",
        "the project whose leftover home entry goes; it must have no record",
    ),
    ("project.list", "store", "list only this Store's projects"),
    (
        "project.stats",
        "branch",
        "the Branch counted; main when left out",
    ),
    (
        "project.export",
        "branch",
        "the Branch exported; main when left out",
    ),
    (
        "project.export",
        "at",
        "export this Commit instead of the Branch's head",
    ),
    (
        "project.doctor",
        "full",
        "also read and check everything its Branches reach; can take minutes",
    ),
    (
        "branch.list",
        "prefix",
        "only Branches whose name starts with this text",
    ),
    (
        "branch.list",
        "exclude_prefix",
        "leave out Branches whose name starts with this text",
    ),
    ("branch.fork", "name", "the new Branch"),
    (
        "branch.fork",
        "from",
        "the Branch forked from; main when left out",
    ),
    (
        "branch.fork",
        "at",
        "fork at this Commit instead of the head of <from>",
    ),
    ("branch.merge", "branch", "the Branch merged into"),
    ("branch.merge", "from", "the Branch merged from"),
    (
        "branch.merge",
        "into",
        "write the result to a new Branch of this name; <branch> stays as it is",
    ),
    (
        "branch.merge",
        "units",
        "merge units as JSON: a list of patterns or {name, patterns}",
    ),
    (
        "branch.publish",
        "at",
        "publish this Commit instead of the Branch's head",
    ),
    (
        "branch.publish",
        "expected",
        "refuse Conflict unless main's head is this Commit",
    ),
    (
        "branch.publish",
        "force",
        "publish over what main gained since the Branch forked",
    ),
    (
        "commit.list",
        "at",
        "start at this Commit instead of the Branch's head",
    ),
    (
        "commit.get",
        "branch",
        "refuse unless the Commit is in this Branch's history",
    ),
    (
        "commit.diff",
        "from",
        "the side compared from; the fork point or the parent when left out",
    ),
    (
        "commit.diff",
        "path",
        "only this file, or this folder and what is under it",
    ),
    ("file.read", "line", "the first line read"),
    ("file.read", "limit", "the most lines read"),
    (
        "file.list",
        "path",
        "an absolute folder in the project; / is the root",
    ),
    ("file.glob", "pattern", "a glob, such as **/*.ts"),
    ("file.glob", "path", "the folder searched; / when left out"),
    (
        "file.grep",
        "pattern",
        "a literal text, not a regular expression",
    ),
    ("file.grep", "path", "the folder searched; / when left out"),
    (
        "file.grep",
        "include",
        "search only paths that match this glob",
    ),
    ("file.grep", "ignore_case", "match letters of either case"),
    (
        "file.history",
        "traversal",
        "provenance follows the path into the Branches it came from; branch stays on this one",
    ),
    ("file.blame", "line", "the first line blamed"),
    ("file.blame", "limit", "the most lines blamed"),
    (
        "file.edit",
        "old_string",
        "the text replaced; it must occur exactly once unless --replace-all",
    ),
    ("file.edit", "new_string", "the text that replaces it"),
    (
        "file.edit",
        "replace_all",
        "replace every occurrence instead of exactly one",
    ),
    (
        "file.rm",
        "recursive",
        "remove a folder with everything in it",
    ),
    (
        "file.mkdir",
        "mode",
        "permission bits of the folder, in octal, such as 0755",
    ),
    ("file.mv", "from", "the absolute path moved"),
    (
        "file.cp",
        "from",
        "the absolute path copied; a folder with everything in it",
    ),
    ("sandbox.create", "name", "the new Sandbox's name"),
    (
        "sandbox.create",
        "machine",
        "the machine it runs on; the Store's machine when left out",
    ),
    (
        "sandbox.create",
        "image",
        "the image it runs; the machine's own when left out",
    ),
    ("sandbox.create", "mount", "also mount this Branch in it"),
    (
        "sandbox.list",
        "project",
        "list only this project's Sandboxes",
    ),
    (
        "sandbox.list",
        "machine",
        "list only this machine's Sandboxes",
    ),
    (
        "sandbox.remove",
        "force",
        "remove it while Workspaces are mounted or commands run; uncommitted changes are lost",
    ),
    (
        "sandbox.files",
        "path",
        "a folder or file of the Sandbox; its roots when left out",
    ),
    ("sandbox.log", "limit", "the most lines in one page"),
    (
        "workspace.list",
        "sandbox",
        "list only this Sandbox's Workspaces",
    ),
    ("workspace.mount", "branch", "a Branch other than main"),
    (
        "workspace.mount",
        "name",
        "the Workspace's name in its Sandbox; the Branch's name when left out",
    ),
    (
        "workspace.exec",
        "cwd",
        "the folder it starts in, from the Workspace's root",
    ),
    (
        "workspace.exec",
        "env",
        "one environment variable, K=V; repeat for more",
    ),
    (
        "workspace.exec",
        "stdin",
        "pass this process's stdin to the command",
    ),
    (
        "workspace.exec",
        "stream",
        "write its output as it comes instead of answering it at the end",
    ),
    (
        "workspace.cancel",
        "exec",
        "end only this command, by its exec id; every running one when left out",
    ),
    (
        "workspace.commit",
        "force",
        "commit over a head that moved since the mount instead of merging with it",
    ),
    (
        "workspace.unmount",
        "discard",
        "drop its uncommitted changes",
    ),
    (
        "workspace.unmount",
        "force",
        "cancel its running commands and abort its connection",
    ),
    (
        "workspace.status",
        "path",
        "list only changes under this folder",
    ),
    (
        "home.init",
        "adopt",
        "write layerfs.json for a home whose data is there but marker is missing; never moves or rewrites the data",
    ),
    (
        "home.log",
        "call",
        "only this call: a family (file) or family.call (file.write)",
    ),
    ("home.log", "project", "only calls on this project"),
    (
        "home.log",
        "client",
        "only calls from this front end: cli, node, python or rust",
    ),
    (
        "home.log",
        "tag",
        "only calls with this log tag, or a prefix ending in :",
    ),
    (
        "home.log",
        "outcome",
        "only calls with this outcome word or refusal code",
    ),
    (
        "home.log",
        "since",
        "only calls at or after this time, in milliseconds since 1970",
    ),
    ("home.log", "limit", "the most lines in one page"),
    (
        "machine.load_image",
        "image",
        "a Docker reference, image id or sha256: digest in the host's Docker",
    ),
    (
        "machine.prune",
        "image",
        "an image the machine holds; left off with --unused",
    ),
    (
        "machine.prune",
        "unused",
        "remove every image no Sandbox uses, except the machine's default",
    ),
    (
        "machine.files",
        "path",
        "a folder or file of the machine; its LayerFS folders when left out",
    ),
    ("machine.log", "limit", "the most lines in one page"),
    (
        "machine.stop",
        "force",
        "stop it and this home's Sandboxes on it without asking its worker; data stays",
    ),
    ("machine.create", "name", "the new machine's name"),
    ("machine.create", "disk", "the VM's data disk, such as 20G"),
    (
        "machine.create",
        "image",
        "the Sandbox image its Sandboxes run: a digest or a file",
    ),
    (
        "machine.create",
        "os_image",
        "the VM's OS image; the provider's own when left out",
    ),
    (
        "machine.update",
        "image",
        "the Sandbox image its Sandboxes run",
    ),
    (
        "machine.update",
        "docker",
        "the docker machine's Engine socket; \"\" chooses it again at the next start",
    ),
    ("machine.update", "wsl", "a Windows host's WSL distribution"),
    (
        "machine.update",
        "daemon",
        "the absolute path of the daemon a built-in machine's Sandboxes run",
    ),
    (
        "store.list",
        "machine",
        "list only the sqlite Stores on this machine",
    ),
    ("store.create", "name", "the new Store's name"),
    (
        "store.create",
        "machine",
        "make it on this machine; native when left out",
    ),
    (
        "store.connect",
        "url",
        "a postgresql:// address with no user or password",
    ),
    (
        "store.connect",
        "auth",
        "the saved login; needed on a Store's first connect",
    ),
    (
        "store.connect",
        "machine",
        "the address is this machine's own",
    ),
    (
        "store.disconnect",
        "machine",
        "remove only this machine's own address",
    ),
    ("store.save_auth", "user", "the PostgreSQL role"),
    (
        "store.save_auth",
        "password",
        "the password, read from stdin",
    ),
    ("store.update", "auth", "another saved login"),
    (
        "store.move",
        "machine",
        "the machine it moves to; native moves it to this computer",
    ),
    (
        "store.doctor",
        "machine",
        "also check a PostgreSQL Store's connection from this machine",
    ),
    ("backup.connect", "name", "the bucket's name in this home"),
    (
        "backup.connect",
        "url",
        "r2://…, or an S3 endpoint http(s)://<host:port>/<bucket>/<prefix>",
    ),
    ("backup.connect", "access_key", "the key's id"),
    (
        "backup.connect",
        "secret_key",
        "the key's secret, read from stdin",
    ),
    ("backup.list", "bucket", "list only this bucket"),
    ("backup.create", "store", "a sqlite Store"),
    (
        "backup.create",
        "file",
        "the file's name in the bucket; the Store's name when left out",
    ),
    (
        "backup.restore",
        "store",
        "the sqlite Store it becomes on this computer",
    ),
    (
        "backup.doctor",
        "full",
        "also write one small object, read it back and delete it",
    ),
];

/// One example command line of each call.
const EXAMPLES: &[(&str, &str)] = &[
    ("home.init", "layerfs home init"),
    ("home.get", "layerfs home get --with platforms"),
    ("home.log", "layerfs home log --call file.write --limit 20"),
    ("home.clear_log", "layerfs home clear-log"),
    ("home.doctor", "layerfs home doctor"),
    (
        "project.init",
        "layerfs project init shop/web ~/code/web --progress",
    ),
    ("project.list", "layerfs project list --store shop"),
    ("project.get", "layerfs project get shop/web"),
    (
        "project.stats",
        "layerfs project stats shop/web --branch work",
    ),
    (
        "project.export",
        "layerfs project export shop/web ~/out/web --branch work",
    ),
    ("project.delete", "layerfs project delete shop/old"),
    ("project.prune", "layerfs project prune shop/orphan"),
    (
        "project.hide_path",
        "layerfs project hide-path shop/web /secrets.env",
    ),
    ("project.doctor", "layerfs project doctor shop/web --full"),
    (
        "branch.list",
        "layerfs branch list shop/web --prefix agent-",
    ),
    ("branch.get", "layerfs branch get shop/web work"),
    ("branch.fork", "layerfs branch fork shop/web work"),
    (
        "branch.merge",
        "layerfs branch merge shop/web work fix --into work-fixed",
    ),
    ("branch.publish", "layerfs branch publish shop/web work"),
    ("branch.remove", "layerfs branch remove shop/web work"),
    (
        "commit.list",
        "layerfs commit list shop/web work --limit 10",
    ),
    ("commit.get", "layerfs commit get shop/web 12df0720"),
    (
        "commit.diff",
        "layerfs commit diff shop/web work --with patch",
    ),
    (
        "file.read",
        "layerfs file read shop/web main /README.md --limit 20",
    ),
    ("file.stat", "layerfs file stat shop/web main /package.json"),
    ("file.list", "layerfs file list shop/web main /src"),
    (
        "file.glob",
        "layerfs file glob shop/web main '**/*.ts' /src",
    ),
    (
        "file.grep",
        "layerfs file grep shop/web main TODO /src --include '*.ts'",
    ),
    (
        "file.history",
        "layerfs file history shop/web work /src/app.ts",
    ),
    ("file.blame", "layerfs file blame shop/web work /src/app.ts"),
    (
        "file.write",
        "echo hi | layerfs file write shop/web work /hello.txt -m \"say hi\"",
    ),
    (
        "file.edit",
        "layerfs file edit shop/web work /app.ts --old-string foo --new-string bar",
    ),
    (
        "file.rm",
        "layerfs file rm shop/web work /build --recursive",
    ),
    ("file.mkdir", "layerfs file mkdir shop/web work /src/lib"),
    ("file.mv", "layerfs file mv shop/web work /old.ts /new.ts"),
    (
        "file.cp",
        "layerfs file cp shop/web work /template /src/page",
    ),
    (
        "file.apply",
        "layerfs file apply shop/web work --from changes.json -m \"refactor\"",
    ),
    (
        "sandbox.create",
        "layerfs sandbox create shop/web box --mount work",
    ),
    ("sandbox.list", "layerfs sandbox list --project shop/web"),
    ("sandbox.get", "layerfs sandbox get box --with stats"),
    ("sandbox.remove", "layerfs sandbox remove box"),
    ("sandbox.files", "layerfs sandbox files box"),
    ("sandbox.log", "layerfs sandbox log box --limit 50"),
    ("sandbox.doctor", "layerfs sandbox doctor box"),
    ("workspace.list", "layerfs workspace list --sandbox box"),
    ("workspace.mount", "layerfs workspace mount box work"),
    (
        "workspace.exec",
        "layerfs workspace exec box:work \"npm test\"",
    ),
    ("workspace.cancel", "layerfs workspace cancel box:work"),
    (
        "workspace.commit",
        "layerfs workspace commit box:work -m \"tests pass\"",
    ),
    ("workspace.unmount", "layerfs workspace unmount box:work"),
    ("workspace.status", "layerfs workspace status box:work"),
    ("machine.list", "layerfs machine list --with live"),
    ("machine.get", "layerfs machine get docker --with live"),
    ("machine.images", "layerfs machine images docker"),
    (
        "machine.load_image",
        "layerfs machine load-image docker node:22-bookworm",
    ),
    ("machine.prune", "layerfs machine prune docker --unused"),
    ("machine.stats", "layerfs machine stats docker --with space"),
    ("machine.files", "layerfs machine files docker"),
    ("machine.log", "layerfs machine log docker --limit 50"),
    ("machine.start", "layerfs machine start docker --progress"),
    ("machine.stop", "layerfs machine stop docker"),
    (
        "machine.create",
        "layerfs machine create dev lima --cpus 4 --memory 8G",
    ),
    ("machine.remove", "layerfs machine remove dev"),
    ("machine.update", "layerfs machine update dev --cpus 8"),
    ("machine.doctor", "layerfs machine doctor docker"),
    ("store.list", "layerfs store list --with stats"),
    ("store.get", "layerfs store get shop --with stats"),
    ("store.create", "layerfs store create shop"),
    (
        "store.connect",
        "layerfs store connect team postgresql://db.local:5432/team --auth admin",
    ),
    ("store.disconnect", "layerfs store disconnect team"),
    (
        "store.save_auth",
        "layerfs store save-auth admin --user layerfs --password-stdin",
    ),
    ("store.auths", "layerfs store auths"),
    ("store.remove_auth", "layerfs store remove-auth admin"),
    (
        "store.update",
        "layerfs store update team --synchronous-commit on",
    ),
    ("store.move", "layerfs store move shop docker --progress"),
    ("store.remove", "layerfs store remove shop"),
    ("store.compact", "layerfs store compact shop --progress"),
    ("store.doctor", "layerfs store doctor shop"),
    (
        "backup.connect",
        "layerfs backup connect r2 r2://my-bucket --access-key KEY --key-stdin",
    ),
    ("backup.disconnect", "layerfs backup disconnect r2"),
    ("backup.list", "layerfs backup list"),
    ("backup.create", "layerfs backup create shop r2"),
    (
        "backup.restore",
        "layerfs backup restore r2 shop shop-restored",
    ),
    ("backup.doctor", "layerfs backup doctor r2 --full"),
];

/// One line on family `family`.
pub fn family(family: &str) -> Option<&'static str> {
    FAMILIES
        .iter()
        .find(|(name, _)| *name == family)
        .map(|(_, words)| *words)
}

/// What argument or option `name` of call `call` (`family.call`) means.
pub fn arg(call: &str, name: &str) -> Option<&'static str> {
    let of = |scope: &str| {
        ARGS.iter()
            .find(|(at, arg, _)| *at == scope && *arg == name)
            .map(|(_, _, words)| *words)
    };
    of(call).or_else(|| of("*"))
}

/// One example command line of call `call` (`family.call`).
pub fn example(call: &str) -> Option<&'static str> {
    EXAMPLES
        .iter()
        .find(|(name, _)| *name == call)
        .map(|(_, line)| *line)
}
