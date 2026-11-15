#!/usr/bin/env bash
# Store tests need a filesystem that carries SQLite's locks.
#
# Why this exists
# ---------------
# A Store opens directly only inside a private (0700) directory, owned by this
# user, on a local filesystem whose locks reach every opener: ext4, xfs or
# btrfs on Linux; apfs or hfs on macOS. Anything else is `Unsupported`
# (engine/crates/layerfs-store-service/src/kernel_guard.rs), because SQLite's
# WAL locks and the writers' directory flock cannot coordinate across kernels
# or on tmpfs, overlay, FUSE or network mounts.
#
# The Store suites place their Stores in `std::env::temp_dir()` (TMPDIR) and in
# `env!("CARGO_TARGET_TMPDIR")` (CARGO_TARGET_DIR/tmp). On a host whose /tmp is
# tmpfs -- or inside a container whose checkout is an overlay -- the guard
# refuses every Store and suites such as `layerfs-store-service --test
# kernel_guard` fail with "does not carry SQLite locks". That is an environment
# failure, not a product failure.
#
# This script requires LAYERFS_TEST_TMPDIR on ext4, xfs or btrfs (apfs or hfs on
# macOS), refuses anything else before cargo starts, and points TMPDIR,
# CARGO_TARGET_DIR and CARGO_HOME at it. See
# docs/architecture/engine/store/testing.md.
#
# Usage
#   LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --quick
#   LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh
#   tools/ci/test_store_ext4.sh --list
#
# Options
#   --quick    a small cross-crate subset (heads, sqlite_publication,
#              kernel_guard, commit_pipeline, create_directory, store_family)
#   --full     every Store suite (the default)
#   --list     print the cargo commands, run nothing
#   --help     this text
#
# Environment
#   LAYERFS_TEST_TMPDIR        required; must be on ext4, xfs or btrfs (Linux)
#                              or apfs or hfs (macOS)
#   LAYERFS_TEST_JOBS          cargo -j; default min(nproc, 16)
#   LAYERFS_TEST_CARGO_HOME    default $LAYERFS_TEST_TMPDIR/cargo-home
#   LAYERFS_TEST_CARGO_TARGET_DIR
#                              default $LAYERFS_TEST_TMPDIR/target
#   LAYERFS_TEST_POSTGRES_URL  libpq connection string; enables the postgresql
#                              feature on every suite and the PostgreSQL cases.
#                              Full-mode diagnostics need amcheck created in
#                              that database (see testing.md); the harness
#                              warns when psql can see that it is missing.
#   LAYERFS_TEST_EXPECT_AMCHECK
#                              assert a Pass relation summary in
#                              postgresql_diagnostic (optional)
#   Arguments after `--` are appended to every `cargo test` (e.g. -- --nocapture).
#
# Exit status: 0 every suite passed; 1 at least one failed; 2 wrong command line
# or a refused LAYERFS_TEST_TMPDIR; 127 cargo is not installed.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

usage() {
  sed -n '2,/^set -euo pipefail$/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
}

refuse() {
  printf 'test_store_ext4: %s\n' "$1" >&2
  shift || true
  for line in "$@"; do printf '%s\n' "$line" >&2; done
  exit 2
}

mode=full
list_only=0
passthrough=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --quick) mode=quick ;;
    --full) mode=full ;;
    --list) list_only=1 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; passthrough=("$@"); break ;;
    *) refuse "unknown argument: $1 (try --help)" ;;
  esac
  shift
done

# ---- The directory and its filesystem -------------------------------------

dir=${LAYERFS_TEST_TMPDIR:-}
if [ -z "$dir" ]; then
  refuse "LAYERFS_TEST_TMPDIR is not set." \
    "" \
    "The Store suites need their temporary Stores on a filesystem that carries" \
    "SQLite's locks: ext4, xfs or btrfs on Linux; apfs or hfs on macOS." \
    "Choose such a directory and re-run, for example:" \
    "" \
    "  LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests $0 --quick" \
    "" \
    "Inside a Docker Sandbox the run directory is ext4 (\$HOME and" \
    "/layerfs/data/machine/native/run/<sandbox>); /tmp is tmpfs there."
fi

case "$dir" in
  /*) ;;
  *) dir="$PWD/$dir" ;;
esac
if command -v realpath >/dev/null 2>&1; then
  dir=$(realpath -m -- "$dir")
fi

mkdir -p -- "$dir" 2>/dev/null || refuse "cannot create LAYERFS_TEST_TMPDIR=$dir"
[ -d "$dir" ] || refuse "LAYERFS_TEST_TMPDIR=$dir is not a directory"

uname_s=$(uname -s 2>/dev/null || printf 'Linux')

filesystem_type() {
  local target=$1 found=
  if [ "$uname_s" = Darwin ]; then
    # macOS: the mount table names the filesystem in parentheses; `stat -f %T`
    # is a fallback for a mount line this cannot parse.
    local device type
    device=$(df -P "$target" 2>/dev/null | awk 'NR == 2 { print $1 }')
    type=$(mount 2>/dev/null | awk -v device="$device" '$1 == device { print; exit }' \
      | sed -n 's/.*(\([^,)]*\).*/\1/p')
    if [ -n "$type" ]; then
      printf '%s\n' "$type"
      return 0
    fi
    stat -f %T "$target" 2>/dev/null || true
    return 0
  fi
  if command -v findmnt >/dev/null 2>&1; then
    found=$(findmnt -n -o FSTYPE --target "$target" 2>/dev/null || true)
    if [ -n "$found" ]; then
      printf '%s\n' "$found"
      return 0
    fi
  fi
  found=$(stat -f -c %T "$target" 2>/dev/null || true)
  case "$found" in ext2/ext3) found=ext4 ;; esac
  printf '%s\n' "$found"
}

if [ "$uname_s" = Darwin ]; then
  supported='apfs hfs'
  supported_text='apfs or hfs'
else
  supported='ext4 xfs btrfs'
  supported_text='ext4, xfs or btrfs'
fi

refuse_filesystem() {
  local target=$1 label=$2 found=$3
  refuse "$label=$target is on '$found', which cannot carry a Store." \
    "" \
    "Direct Store access needs a local filesystem that carries SQLite's locks:" \
    "$supported_text. The kernel guard refuses tmpfs, overlay, FUSE and network" \
    "mounts before any Store file is opened, so the suites would fail with" \
    "'does not carry SQLite locks' -- an environment failure, not a product one." \
    "" \
    "Point LAYERFS_TEST_TMPDIR at a supported directory, for example:" \
    "  LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests $0 --quick" \
    "" \
    "Inside a Docker Sandbox the run directory is ext4 (\$HOME and" \
    "/layerfs/data/machine/native/run/<sandbox>); /tmp is tmpfs there."
}

check_supported() {
  local target=$1 label=$2 found
  found=$(filesystem_type "$target")
  case " $supported " in
    *" ${found:-unknown} "*) ;;
    *) refuse_filesystem "$target" "$label" "${found:-unknown}" ;;
  esac
}

check_supported "$dir" LAYERFS_TEST_TMPDIR

probe="$dir/.layerfs-store-tests.$$"
if ! ( : > "$probe" ) 2>/dev/null; then
  refuse "LAYERFS_TEST_TMPDIR=$dir is not writable by $(id -un 2>/dev/null || echo this user)."
fi
rm -f -- "$probe"

# ---- Point the tests at it ------------------------------------------------

export TMPDIR="$dir/tmp"
export CARGO_TARGET_DIR="${LAYERFS_TEST_CARGO_TARGET_DIR:-$dir/target}"
export CARGO_HOME="${LAYERFS_TEST_CARGO_HOME:-$dir/cargo-home}"
mkdir -p -- "$TMPDIR" "$CARGO_TARGET_DIR" "$CARGO_HOME" 2>/dev/null \
  || refuse "cannot create TMPDIR, CARGO_TARGET_DIR and CARGO_HOME under $dir"

# Unix-domain sockets live under TMPDIR, and Linux rejects a socket path of 108
# bytes or more. A Sandbox run directory is long enough that its own path plus
# a test directory would overrun that, so TMPDIR itself is short: a private
# wrapper under the system temporary directory whose `tmp` entry is a symlink
# to the real one. statfs follows the symlink, so the Stores still land on
# `$dir` filesystem; the wrapper holds nothing but the link.
system_tmp=${LAYERFS_TEST_SYSTEM_TMP:-/tmp}
if [ -d "$system_tmp" ] && [ -w "$system_tmp" ]; then
  tmp_wrapper=$(mktemp -d "$system_tmp/layerfs-store-tests.XXXXXX") \
    || refuse "cannot create a short TMPDIR wrapper under $system_tmp"
  ln -s "$dir/tmp" "$tmp_wrapper/tmp" \
    || { rm -rf -- "$tmp_wrapper"; refuse "cannot link $system_tmp/.../tmp to $dir/tmp"; }
  export TMPDIR="$tmp_wrapper/tmp"
  trap 'if [ -n "${tmp_wrapper:-}" ]; then rm -rf -- "$tmp_wrapper"; fi' EXIT
else
  export TMPDIR="$dir/tmp"
fi

# `CARGO_TARGET_TMPDIR` follows CARGO_TARGET_DIR, and the tests also read TMPDIR:
# both must be on the supported filesystem, including when they are overridden.
check_supported "$TMPDIR" TMPDIR
check_supported "$CARGO_TARGET_DIR" CARGO_TARGET_DIR

# The pinned fuser dependency is fetched over git. The git CLI honours the
# host's proxy and credential configuration where libgit2 does not, and some
# TLS-inspecting hosts need verification off for that fetch. Scoped to this
# process; not a production setting.
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export GIT_SSL_NO_VERIFY=true

# The official Rust image keeps rustup and the toolchain outside HOME.
if [ -z "${RUSTUP_HOME:-}" ] && [ -d /usr/local/rustup ] && [ -x /usr/local/cargo/bin/rustup ]; then
  export RUSTUP_HOME=/usr/local/rustup
fi

if ! command -v cargo >/dev/null 2>&1; then
  for candidate in "$CARGO_HOME/bin" "${HOME:-}/.cargo/bin" /usr/local/cargo/bin; do
    if [ -x "$candidate/cargo" ]; then
      PATH="$candidate:$PATH"
      export PATH
      break
    fi
  done
fi
if ! command -v cargo >/dev/null 2>&1; then
  refuse "cargo is not installed or not on PATH; install Rust 1.85.1 (rustup) first."
fi

cargo_cmd=(cargo)
if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q '^1\.85\.1'; then
  cargo_cmd=(cargo +1.85.1)
fi

if [ -n "${LAYERFS_TEST_JOBS:-}" ]; then
  case "$LAYERFS_TEST_JOBS" in
    ''|*[!0-9]*) refuse "LAYERFS_TEST_JOBS must be a positive integer, not '$LAYERFS_TEST_JOBS'." ;;
  esac
  jobs=$LAYERFS_TEST_JOBS
else
  jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '4')
  if [ "$jobs" -gt 16 ]; then
    jobs=16
  fi
fi
[ "$jobs" -ge 1 ] || refuse "LAYERFS_TEST_JOBS must be a positive integer, not '$jobs'."

# ---- The suites -----------------------------------------------------------

ran=0
failed=0

show_command() {
  printf '   '
  printf ' %q' "$@"
  printf '\n'
}

run_cargo() {
  local package=$1
  shift
  local -a cmd=( "${cargo_cmd[@]}" test --locked -j "$jobs" -p "$package" )
  if [ -n "${LAYERFS_TEST_POSTGRES_URL:-}" ]; then
    cmd+=( --features postgresql )
  fi
  cmd+=( "$@" )
  if [ "${#passthrough[@]}" -gt 0 ]; then
    cmd+=( -- "${passthrough[@]}" )
  fi
  show_command "${cmd[@]}"
  if [ "$list_only" -eq 1 ]; then
    return 0
  fi
  printf '\n==> %s:%s\n' "$package" "$*"
  ran=$((ran + 1))
  if "${cmd[@]}"; then
    printf 'PASS %s\n' "$package"
  else
    failed=$((failed + 1))
    printf 'FAIL %s\n' "$package" >&2
  fi
}

# ---- The PostgreSQL prerequisites -----------------------------------------
#
# The full-mode doctor calls amcheck's verify_heapam and bt_index_check on
# every heap and ready B-tree relation, and the PostgreSQL compaction suites
# assert a clean full audit. A database without amcheck reports
# store.database.relations=Unknown, so those tests fail with a diagnostic
# finding, not an environment refusal. The engine never installs the
# extension: warn, never fail, so the SQLite-only lane and a deliberately
# amcheck-free server stay usable. psql is optional too; without it the check
# is skipped with a clear message.
postgresql_extension_count() {
  psql "$LAYERFS_TEST_POSTGRES_URL" -X -A -t -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM pg_catalog.pg_extension WHERE extname='$1'" 2>/dev/null
}

warn_about_postgresql_prerequisites() {
  if ! command -v psql >/dev/null 2>&1; then
    printf '%s\n' \
      "test_store_ext4: LAYERFS_TEST_POSTGRES_URL is set but psql is not on PATH;" \
      "  the amcheck precondition was not checked. Full-mode diagnostics report" \
      "  store.database.relations=Unknown without it. Create it out of band:" \
      "    CREATE EXTENSION IF NOT EXISTS amcheck;" >&2
    return 0
  fi
  local extension count
  for extension in amcheck pgstattuple; do
    if ! count=$(postgresql_extension_count "$extension"); then
      printf 'test_store_ext4: cannot query LAYERFS_TEST_POSTGRES_URL; the %s precondition was not checked.\n' "$extension" >&2
      return 0
    fi
    if [ "${count:-0}" -gt 0 ]; then
      continue
    fi
    if [ "$extension" = amcheck ]; then
      printf '%s\n' \
        "test_store_ext4: warning: the PostgreSQL test database has no amcheck extension." \
        "  The full-mode doctor reports store.database.relations=Unknown and the" \
        "  PostgreSQL compaction suites fail their clean-audit assertions (T-G14)." \
        "  Create it in that database, then re-run:" \
        "    psql \"\$LAYERFS_TEST_POSTGRES_URL\" -c 'CREATE EXTENSION IF NOT EXISTS amcheck;'" >&2
    else
      printf 'test_store_ext4: note: %s is not installed (companion extension; amcheck is the required one).\n' "$extension" >&2
    fi
  done
  return 0
}

printf '%s\n' \
  "Store test harness" \
  "  repository:          $root" \
  "  LAYERFS_TEST_TMPDIR: $dir ($(filesystem_type "$dir"))" \
  "  TMPDIR:              $TMPDIR" \
  "  CARGO_TARGET_DIR:    $CARGO_TARGET_DIR" \
  "  CARGO_HOME:          $CARGO_HOME" \
  "  jobs (-j):           $jobs" \
  "  mode:                $mode" \
  "  PostgreSQL:          $([ -n "${LAYERFS_TEST_POSTGRES_URL:-}" ] && echo 'on (LAYERFS_TEST_POSTGRES_URL set)' || echo 'off (LAYERFS_TEST_POSTGRES_URL unset; PostgreSQL cases print NOT_RUN)')"

if [ -n "${LAYERFS_TEST_POSTGRES_URL:-}" ] && [ "$list_only" -eq 0 ]; then
  warn_about_postgresql_prerequisites
fi

if [ "$mode" = quick ]; then
  run_cargo layerfs-persistence --test heads --test sqlite_publication
  run_cargo layerfs-store-service --test kernel_guard --test commit_pipeline
  run_cargo layerfs-store --test create_directory
  run_cargo layerfs-api --test store_family
else
  run_cargo layerfs-persistence
  run_cargo layerfs-store-service
  run_cargo layerfs-store
  api_store_targets=()
  for file in api/crates/layerfs-api/tests/store_*.rs; do
    [ -f "$file" ] || continue
    target=${file##*/}
    api_store_targets+=( --test "${target%.rs}" )
  done
  [ "${#api_store_targets[@]}" -gt 0 ] \
    || refuse "no api/crates/layerfs-api/tests/store_*.rs targets found; is $root the repository root?"
  run_cargo layerfs-api "${api_store_targets[@]}"
fi

if [ "$list_only" -eq 1 ]; then
  exit 0
fi

printf '\n%d suite(s), %d failed\n' "$ran" "$failed"
if [ "$failed" -ne 0 ]; then
  exit 1
fi
