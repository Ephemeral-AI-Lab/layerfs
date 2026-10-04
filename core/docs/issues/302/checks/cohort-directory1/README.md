# Within-cohort full-directory validation

Count/cause: before this treatment, the whole-pack loop invokes `group_view`
for each requested group, and `group_view` visits every directory entry. Fresh
whole admission and its post-admission presence check require 2N walks of G
entries; a cached N-group cohort requires N walks. The same immutable body and
header apply throughout each boundary. This is source-derived work algebra,
not an old/new clock comparison or a claim about SQLite mapping executions.

The treatment shares only the full-directory walk within that cohort. The public
cache fixture's eight-group demand records 2 walks / 16 equivalent unshared
walks at admission, then 1 / 8 at a subsequent cohort. Descriptor checks remain
8 at each boundary. All ordinary extent/codec/continuity checks and compact
directory checks remain; corruption in an unrequested entry is refused. Single
access and selected-unit authentication are unchanged. No memo or view survives
a request, no output/body/cache/transaction limit changes, and no allocation is
added to directory validation. The three real diagnostic u64 fields add 24 bytes
per existing Diagnostics value and retain no input data.

Covering tests, locked Clippy, formatting and boundary checks are recorded in
[checks.json](checks.json). No new performance claim precedes the matched pair.
