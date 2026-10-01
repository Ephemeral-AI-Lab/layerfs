# JuiceFS DeepSeek full repository upload experiment v1

Status: Research; informative and not a product contract.
Owner request 2026-10-01. Issue https://github.com/Ephemeral-AI-Lab/layerfs/issues/292.
Unmodified upstream fork https://github.com/Ephemeral-AI-Lab/juicefs;
local clone /Users/yifanxu/Ephemeral-AI-Lab/juicefs at
adcca1cc61bb4d668a945d64b2e176b44ac8e5b5. No third-party production edits or
LayerFS dependency substitution. LayerFS parent8c295e99f7bee3bb2915a5c205a8e5d7f64e0ad4.

## Selection and topology

J-deepseek-full-upload-v1 attempts all closed corpus entries once using native
macOS JuiceFS1.4.1 public sync through jfs://, host SQLite metadata and native
host MinIO. No mount, Docker, sandbox or #288 campaign. Source release commit
0b90c7db5a929ae6adc5faad948d108efd2c99f9; official darwin-arm64 archive SHA256
e77739762a94e6ad90d27a97027de214d63516828e90fac86221c88850b82b08.
MinIO binary SHA256b107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df.

Reuse the qualified private closed master from#291 at
/Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-master-v2.
103108 regular files,16868 directories including root,10070 symlinks,
3475776149 regular-file bytes,0 hardlink aliases,130046 captured opaque xattrs.
Manifest SHA256541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af.
No filtering of .git, hidden or ignored/generated entries. Symlinks copied as
links, never followed. No source/master edits, fresh acquisition or APFS cloning.

Format one fresh volume with --storage minio, --compress zstd, --block-size 4M,
private generated credentials, no data-cache priming. SQLite uses upstream's
native selected profile; record actual persisted settings without modifying
third-party code. No LayerFS crash-durability claim. Format/readiness and one
separate tiny synthetic transport/symlink qualification are setup, not throughput.

Whole-command timed operation:
juicefs sync --threads 1 --list-threads 1 --dirs --perms --links MASTER/tree/ jfs://juiceprobe/
The juiceprobe environment variable selects the owned SQLite database.
One file-copy producer. Upstream jfs adapter owns its own upload/download/retry
(MaxUpload50, MaxDownload200, MaxRetries10) and300MiB buffer defaults; source-capture and actual provider configuration are
recorded. No concealed equality with LayerFS's bounded C1/C2 profile or four
upload connections. Do not add helpers or increase workers/buffers after a miss.

The timer starts immediately before process launch and ends at exit, including
source enumeration, reading, compression, filesystem metadata, object upload,
client initialization and shutdown. No candidate construction is moved into setup.
No additional benchmark hashes/oracle work inside this timer. Full command limit
25s, an explicitly selected large-corpus exception to15s. One attempt; no resample,
shrinking corpus, resume/adoption on guessed outcomes or increased deadline.
A timeout remains TIMEOUT/PARTIAL and does not authorize a same-arm continuation.
Any independently necessary correctness diagnostic requires its own frozen scope.

## Proof, observations and retained outcomes

After a successful command, separate10s proof attempts public sync reconstruction
into a fresh local directory, with exact fixture-driven regular bytes/EOF, names,
symlink targets and portable mode checks. Retain exact progress and failed bound;
never claim count-only/full-byte PASS. After timeout, record prefix/metadata/object
counts with separate bounded read-only diagnostics, not adoption as completed
upload or new speed sample. Full byte proof remains NOT_RUN when upload is incomplete.

The CLI preserves supported permissions and links. Darwin opaque xattrs/flags,
ACLs and ownership fidelity are not established by this route and must be reported
unsupported/unverified rather than supplied by a benchmark sidecar. Imported data
stays private; publish only aggregate counts, hashes, source/CLI identities and
sanitized logs. Do not publish paths, captured bodies, private credentials or
metadata dumps. Record complete source/command seals and every failing line.

Cache residency unknown, original/master/OS caches may serve reads. Numerical
verdict INELIGIBLE, performance_claim=false. No physical memory/file-cache/IO or
power-loss proof. Storage accounting uses actual object sizes and database/data
file lengths; physical disk allocation is separate. SQLite/Go heaps are not inferred
from configured buffers or lifetime peaks. All missing quantities remain unavailable.

Temporary disk admission16GiB before effects. Retain imported private metadata and
MinIO bytes even after timeout. Cleanup closes/reaps only experiment-owned workers
and stops the owned MinIO process, separately bounded10s. No deletion of other
worktrees or #291 providers/artifacts; no source builds in a foreign Cargo target.

Compare scope honestly with#291's103108-file import: its34.543s construction plus
4.636s prepared-pack transfer are separate stages and an informative sum, not a
matched complete-command arm. Different native metadata, retry, construction and
resource profiles prohibit a qualified speedup ratio. This experiment can report
its own completion, wall, observed counts and scope; it cannot decide cold speed,
LayerFS release admission, CAS/CDC/delta capability or FUSE performance.
