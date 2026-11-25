# Store-format fixtures (#804)

Stores written by a real old LayerFS build, for the `layerfs store upgrade`
steps to be tested against. Never faked from a current Store.

- `sqlite/v2/store.sqlite`: SQLite schema 2 (`PRAGMA user_version` 42), the
  closed file the old build's `store move <store> native` left.
- `postgresql/v3/store.sql`: PostgreSQL schema 3, a plain `pg_dump` of the
  Store's schema `layerfs`; restore it into a fresh database with
  `psql -v ON_ERROR_STOP=1 -f store.sql` (no roles or extensions needed).
- `manifest.json` beside each: the old build's answers at capture (every
  Branch's Commits and every file at its head, with sha256) and the scenario
  seed.

Captured by `tools/store_format.py capture` with runtime `2.0.0-3f9c8e680bca`
(commit `dcd0fa2c301b`, features machine,postgresql,tls,sandbox-runtime).

These files are frozen: never edit them. `SHA256SUMS` lists their hashes and
the frozen guard (`python3 tools/store_format.py verify-hashes`) checks it. A
new format adds a new folder (`tools/store_format.py new-step`).
