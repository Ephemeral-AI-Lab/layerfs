# Directory entries and operation records

> **Status:** Current source terminology following owner direction2026-10-07.

A `directory_entry` is a directory name-to-inode binding, including a recorded
removal. The Overlay public DirectoryEntry and its windows/changes describe the
same concept. The SQL table, capture index, accounting triggers, prepared
identifiers, source/captured read methods, counters and daemon responses use
that terminology. Bare component bytes remain `name`/PathName; kernel/VFS dentry
terminology remains external terminology.

An `operation_record` is an intermediate record owned by a Workspace operation.
The three existing access/ownership shapes retain their distinctions:

| SQL relation | Public shape | Existing ownership |
| --- | --- | --- |
| operation_record | OperationRecord, integer key | Explicit operation lease |
| owned_operation_record | OperationRecord, integer key | Engine-minted OperationOwner |
| indexed_operation_record | IndexedOperationRecordScope/Key/Change/Apply | OperationOwner plus full file scope/kind/binary key |

Workspace uses OverlayOperationRecords and typed OperationRecord replies,
refusals and copy observations. Daemon routes IndexedOperationRecord jobs through
the same fair service class/lane positions. Module filenames and function names
match those concepts. No keys, algorithms, limits or release prerequisites change.
Qualification and construction still use these rows in the one daemon-local
overlay. All Workspaces still share physical tables partitioned by `ns`; no
per-Workspace table or database design is introduced.

The physical SQL name change advances user_version from15 to16. The same
application id and MEMORY/OFF/EXCLUSIVE profile remain. Overlay creates a fresh
owned file with create_new and verifies schema16. Existing paths are refused;
there is no implicit migration, reopen, conversion or retry.

Generic Content sorting/decoder heap scratch, native crypto/frame buffers and
filesystem temporary scratch directories are different concepts and keep their
terminology. Historical source snapshots, raw receipts and E04 schema parsers
also keep the vocabulary of their recorded identity. The previous49 document
path remains as a historical link to the current operation-record description.

[The naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md) records
file planning, exact SQL equivalence under names/version substitutions, runtime
checks, failures and source/LOC identities. This is a naming change and supplies
no new storage-layout, performance, FUSE or complete Commit qualification.
