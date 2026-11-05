# Coverage

<!-- BEGIN GENERATED: api/scripts/gen-coverage.mjs -->

61 calls in 9 families, counted from the Node table. Calls present: Node 61, Python 61, Python `aio` 61, CLI 61.

Argument names that are Python keywords: none.

Front-end problems: none.

| Call | Positionals (`?` optional) | Options (`()` a function) | Node | Python | Python `aio` | CLI |
| --- | --- | --- | --- | --- | --- | --- |
| `project.init` | project, folder | timeout, on_progress() | yes | yes | yes | yes (--progress) |
| `project.list` | ?store | page_size, page_pos | yes | yes | yes | yes |
| `project.get` | project | - | yes | yes | yes | yes |
| `project.remove` | project | - | yes | yes | yes | yes |
| `branch.list` | project | page_size, page_pos | yes | yes | yes | yes |
| `branch.get` | project, branch | - | yes | yes | yes | yes |
| `branch.fork` | project, name, from_branch, ?from_commit | - | yes | yes | yes | yes |
| `branch.merge` | project, name, a, b | message, units | yes | yes | yes | yes |
| `branch.publish` | project, branch, ?commit | - | yes | yes | yes | yes |
| `branch.diff` | project, from_branch, to_branch | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes |
| `commit.list` | project, branch | page_size, page_pos | yes | yes | yes | yes |
| `commit.get` | project, branch, commit | - | yes | yes | yes | yes |
| `commit.diff` | project, from_commit, to_commit | paths_only, page_size, page_pos, cursor | yes | yes | yes | yes |
| `file.read` | project, branch, path, ?commit | start_line, end_line | yes | yes | yes | yes |
| `file.stat` | project, branch, path, ?commit | - | yes | yes | yes | yes |
| `file.list` | project, branch, path, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.glob` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.grep` | project, branch, path, pattern, ?commit | page_size, page_pos | yes | yes | yes | yes |
| `file.blame` | project, branch, path, ?commit | from_line | yes | yes | yes | yes |
| `file.diff` | project, left_branch, left_path, right_branch, right_path | left_commit, right_commit | yes | yes | yes | yes |
| `file.write` | project, branch, path, content | message, mode | yes | yes | yes | yes (stdin/--from) |
| `file.edit` | project, branch, path | old, new, message | yes | yes | yes | yes |
| `file.rm` | project, branch, path | message | yes | yes | yes | yes |
| `file.mkdir` | project, branch, path | message, mode | yes | yes | yes | yes |
| `file.mv` | project, branch, from_path, to_path | message | yes | yes | yes | yes |
| `file.cp` | project, branch, from_path, to_path | message | yes | yes | yes | yes |
| `file.apply` | project, branch, changes | message | yes | yes | yes | yes (stdin/--from) |
| `sandbox.create` | project, name, ?machine | image | yes | yes | yes | yes |
| `sandbox.list` | ?project | page_size, page_pos | yes | yes | yes | yes |
| `sandbox.get` | name | page_size, page_pos | yes | yes | yes | yes |
| `sandbox.close` | name | force | yes | yes | yes | yes |
| `workspace.mount` | sandbox, branch | name | yes | yes | yes | yes |
| `workspace.exec` | ws, command | message, stream, stdout(), stderr() | yes | yes | yes | yes (--stream) |
| `workspace.commit` | ws | message, force | yes | yes | yes | yes |
| `workspace.unmount` | ws | discard, force | yes | yes | yes | yes |
| `workspace.status` | ws | - | yes | yes | yes | yes |
| `machine.list` | - | - | yes | yes | yes | yes |
| `machine.platforms` | - | - | yes | yes | yes | yes |
| `machine.get` | name | - | yes | yes | yes | yes |
| `machine.status` | name | - | yes | yes | yes | yes |
| `machine.start` | name | - | yes | yes | yes | yes |
| `machine.stop` | name | - | yes | yes | yes | yes |
| `machine.create` | name, platform | cpus, memory, disk, image, nested, host | yes | yes | yes | yes |
| `machine.connect` | name, host | user, port, identity, known_hosts, root | yes | yes | yes | yes |
| `machine.remove` | name | - | yes | yes | yes | yes |
| `machine.disconnect` | name | - | yes | yes | yes | yes |
| `machine.update` | name | cpus, memory, image, docker, wsl, daemon | yes | yes | yes | yes |
| `store.list` | - | - | yes | yes | yes | yes |
| `store.get` | name | - | yes | yes | yes | yes |
| `store.status` | name | - | yes | yes | yes | yes |
| `store.create` | name | - | yes | yes | yes | yes |
| `store.connect` | name, url | password, synchronous_commit | yes | yes | yes | yes |
| `store.push` | name, machine | on_progress() | yes | yes | yes | yes (--progress) |
| `store.pull` | name | on_progress() | yes | yes | yes | yes (--progress) |
| `store.remove` | name | - | yes | yes | yes | yes |
| `store.disconnect` | name | - | yes | yes | yes | yes |
| `backup.connect` | name, url | key, region, part_size | yes | yes | yes | yes |
| `backup.disconnect` | name | - | yes | yes | yes | yes |
| `backup.list` | ?bucket | - | yes | yes | yes | yes |
| `backup.create` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) |
| `backup.restore` | store, bucket, ?file | on_progress() | yes | yes | yes | yes (--progress) |

<!-- END GENERATED -->
