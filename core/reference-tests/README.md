# Historical test vectors

> **Status:** Archived; retained for historical evidence only.

These tests describe retired native CAS/private-save and PostgreSQL/MinIO
interfaces. Their original executable product trees remain in Git and in the
sealed Phase 4.5 checkout. They are not active workspace coverage and must not
be counted as passing tests of the new persistence implementation.

C5 transition/identity/pagination/refusal vectors moved into the active
`layerfs-persistence/tests` directory and now exercise its public shared
provider. Pure codec/framing/hash tests remain in `layerfs-storage/tests`.
The new atomic publication, recovery, namespace oracle and entry-count tests
cover their stated scopes; additional retained-history and large-tier
qualification remains required. Retirement is not an algorithmic simplification.
