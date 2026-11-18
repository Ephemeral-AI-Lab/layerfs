#!/usr/bin/env bash
# GC capacity measurement entry point (GC spec section 13 / item-44).
#
#   small   default: ~28 MB live, 4 projects x 4 Layers, 3 runs per arm
#   medium  opt-in: ~9.8 GB live, 16 projects x 52 Layers, 3 runs per arm
#   final   owner-only: ~45.6 GB live / ~50 GB pack, 32 projects x 124 Layers,
#           1 run per arm; NOT run by CI or agents
#
# The harness is `engine/crates/layerfs-persistence/examples/gc_bench.rs`; its
# fixture (`examples/gc_bench/gc_fixture.rs`) publishes real Layer history,
# content shared across projects and delta chains kept as bases. It reports
# downtime, the mark/build/verify/install phases, peak temp bytes, bytes
# written, per-GB rates, 3-run variance, cache state/method and the host class.
# Read `tools/ci/gc-bench.md` for the representativeness caveat, the cold-cache
# method and the fixture shape. Only the Disposable (WAL/synchronous=OFF)
# profile is built, per owner direction 2026-10-07.
#
# Usage:
#   tools/ci/gc_capacity_final.sh [small|medium|final] [--cold-cache]
#       [--cold-method drop_caches|cgroup-memory] [--out FILE] [--runs N]
#       [--check]
#
# Environment:
#   TMPDIR / GC_BENCH_TMPDIR       scratch filesystem (>=3.4x the live bytes)
#   CARGO_HOME                     default /tmp/cargo
#   CARGO_TARGET_DIR               default /tmp/target-gc-bench
#   GC_BENCH_JOBS                  cargo -j, default 8
#   GC_BENCH_HOST_CLASS            declared host class, e.g.
#                                  "Dell PowerEdge R730 / PERC / rotational (not NVMe)"
#   GC_BENCH_NOMINAL_READ_MBS      declared sequential read rate
#   GC_BENCH_NOMINAL_WRITE_MBS     declared sequential write rate
#   GC_BENCH_COLD_METHOD           default cold-cache method (drop_caches)
#   GC_BENCH_MEDIUM=1              required to run the 10 GB arm
#   GC_BENCH_OWNER=1               required to run the 50 GB arm
#   GC_BENCH_OWNER_APPROVED=1      required to run the 50 GB arm
#   GC_BENCH_FINAL_ACK=1           required to run the 50 GB arm
#
# The 50 GB arm is explicitly owner-only. This script is expected to be
# invoked with `final` only by the owner; it refuses and prints the command
# otherwise. It was NOT run for this change, and no 50 GB number was produced.
set -euo pipefail

usage() {
    sed -n '2,62p' "$0" | sed 's/^# \{0,1\}//'
}

mode="small"
out=""
cold=0
cold_method=""
check=0
keep=0
runs=""
while (($#)); do
    case "$1" in
        small|medium|final)
            mode="$1"
            shift
            ;;
        --cold-cache)
            cold=1
            shift
            ;;
        --cold-method)
            cold_method="${2:?--cold-method needs drop_caches or cgroup-memory}"
            shift 2
            ;;
        --keep)
            keep=1
            shift
            ;;
        --out)
            out="${2:?--out needs a path}"
            shift 2
            ;;
        --runs)
            runs="${2:?--runs needs a number}"
            shift 2
            ;;
        --check)
            check=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "gc_capacity_final: unknown argument $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

export PATH="/usr/local/cargo/bin:${PATH:-}"
export RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}"
export CARGO_HOME="${CARGO_HOME:-/tmp/cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/target-gc-bench}"
export TMPDIR="${GC_BENCH_TMPDIR:-${TMPDIR:-/tmp}}"
export CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}"
jobs="${GC_BENCH_JOBS:-8}"
cold_method="${cold_method:-${GC_BENCH_COLD_METHOD:-}}"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"

# Declared host class: the off-host Linux container does not expose DMI or
# /sys/block, so the owner declares it here (or in the environment). Never
# infer NVMe or a controller from a container's virtual devices.
if [[ -z "${GC_BENCH_HOST_CLASS:-}" ]]; then
    product="$(cat /sys/class/dmi/id/product_name 2>/dev/null || true)"
    case "$product" in
        *R730*)
            export GC_BENCH_HOST_CLASS="Dell PowerEdge R730 / PERC / rotational (not NVMe)"
            export GC_BENCH_NOMINAL_READ_MBS="${GC_BENCH_NOMINAL_READ_MBS:-500}"
            export GC_BENCH_NOMINAL_WRITE_MBS="${GC_BENCH_NOMINAL_WRITE_MBS:-600}"
            ;;
        *)
            echo "gc_capacity_final: warning: host class not declared; set" >&2
            echo "  GC_BENCH_HOST_CLASS='Dell PowerEdge R730 / PERC / rotational (not NVMe)'" >&2
            echo "  GC_BENCH_NOMINAL_READ_MBS=500 GC_BENCH_NOMINAL_WRITE_MBS=600" >&2
            ;;
    esac
fi

available_bytes() {
    df -P -B1 "$1" | awk 'NR==2 {print $4}'
}

require_space() {
    local need="$1" avail
    avail="$(available_bytes "$TMPDIR")"
    if [[ -z "$avail" || "$avail" -lt "$need" ]]; then
        echo "gc_capacity_final: not enough free space at $TMPDIR" >&2
        echo "  available=${avail:-unknown} required=${need}" >&2
        exit 3
    fi
    echo "gc_capacity_final: $TMPDIR free=$((avail / 1024 / 1024 / 1024)) GiB required=$((need / 1024 / 1024 / 1024)) GiB"
}

# Refuse non-default arms before compiling anything. The 50 GB arm is
# owner-only by construction: three independent gates, and it is NOT run for
# item-44.
if ((!check)) && [[ "$mode" == "medium" && "${GC_BENCH_MEDIUM:-}" != "1" ]]; then
    echo "gc_capacity_final: the 10 GB medium arm is opt-in; set GC_BENCH_MEDIUM=1" >&2
    echo "  (it needs a TMPDIR with >=34 GiB free and takes minutes to hours)" >&2
    exit 3
fi
if ((!check)) && [[ "$mode" == "final" ]] \
    && [[ "${GC_BENCH_OWNER:-}" != "1" \
        || "${GC_BENCH_OWNER_APPROVED:-}" != "1" \
        || "${GC_BENCH_FINAL_ACK:-}" != "1" ]]; then
    echo "gc_capacity_final: the 50 GB final arm is owner-only and was NOT run." >&2
    echo "  The owner must set all three gates:" >&2
    echo "    GC_BENCH_OWNER=1 GC_BENCH_OWNER_APPROVED=1 GC_BENCH_FINAL_ACK=1" >&2
    echo "  and then run:" >&2
    echo "    GC_BENCH_OWNER=1 GC_BENCH_OWNER_APPROVED=1 GC_BENCH_FINAL_ACK=1 \\" >&2
    echo "      tools/ci/gc_capacity_final.sh final --out /path/to/final.json" >&2
    exit 3
fi

# Build once; every mode uses the same release example binary.
echo "gc_capacity_final: building examples/gc_bench (release, -j $jobs)"
cargo build -p layerfs-persistence --release --example gc_bench -j "$jobs"
bin="$CARGO_TARGET_DIR/release/examples/gc_bench"
test -x "$bin" || { echo "gc_capacity_final: missing $bin" >&2; exit 1; }

if ((check)); then
    "$bin" --scale small --dry-run >/dev/null
    "$bin" --scale medium --dry-run >/dev/null
    "$bin" --scale final --dry-run >/dev/null
    echo "gc_capacity_final: check passed (built, dry-run plans for small/medium/final)"
    exit 0
fi

case "$mode" in
    small)
        require_space $((2 * 1024 * 1024 * 1024))
        scale=small
        default_runs=3
        need=$((2 * 1024 * 1024 * 1024))
        ;;
    medium)
        scale=medium
        default_runs=3
        need=$((34 * 1024 * 1024 * 1024))
        require_space "$need"
        ;;
    final)
        scale=final
        default_runs=1
        need=$((150 * 1024 * 1024 * 1024))
        require_space "$need"
        ;;
esac

if [[ -z "$out" ]]; then
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    out="$TMPDIR/gc-bench-${scale}-${stamp}.json"
fi
mkdir -p "$(dirname -- "$out")"

args=(--scale "$scale" --out "$out")
if [[ -n "$runs" ]]; then
    args+=(--runs "$runs")
else
    args+=(--runs "$default_runs")
fi
((cold)) && args+=(--cold-cache)
if [[ -n "$cold_method" ]]; then
    args+=(--cold-method "$cold_method")
fi
((keep)) && args+=(--keep)
if [[ "$scale" == "final" ]]; then
    args+=(--owner-approved)
fi

echo "gc_capacity_final: running $bin ${args[*]}"
"$bin" "${args[@]}"

python3 - "$out" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)
host = data["host"]
print(f"gc_capacity_final: wrote {path}")
print(f"  scale={data['scale']} runs/arm={data['runsPerArm']} profile={data['profile']}")
print(f"  host={host['hostClass']} source={host['hostClassSource']} "
      f"media={host['storageMedia']} notNvme={host['notNvme']} "
      f"nominalMB/s={host['nominalSequentialMBs']}")
fixture = data["fixture"]
print(f"  fixture: {fixture['stacks']} projects x {fixture['layersPerStackMin']}-"
      f"{fixture['layersPerStackMax']} Layers, {fixture['historyLayers']} Layers / "
      f"{fixture['historyCommits']} Commits, {fixture['sharedObjects']} shared objects "
      f"({fixture['sharedReferences']} refs), {fixture['deltaChains']} delta chains "
      f"depth {fixture['deltaChainMaxDepth']}, {fixture['liveObjects']} live objects")
print(f"  cache: method={data['cache']['method']} state={data['cache']['state']} "
      f"applied={data['cache']['applied']} refused={data['cache']['refused']}")
for arm in ("none", "rebuild"):
    a = data["arms"][arm]
    g = a["perGb"]
    v = a["variance"]
    print(f"  {arm}: downtime={a['median']['downtimeSeconds']:.3f}s "
          f"range=[{a['minWallMicros']/1e6:.3f}, {a['maxWallMicros']/1e6:.3f}]s "
          f"stddev={v['wallSecondsStdDev']:.3f}s rel={v['wallRelativeRange']:.4f} "
          f"s/GB-live={g['wallSecondsPerLiveGb']:.2f} "
          f"s/GB-freed={g['wallSecondsPerFreedGb']:.2f} "
          f"peakTemp={a['median']['peakTempBytes']} "
          f"write={a['median']['processWriteBytes']}B")
PY

if [[ "$scale" == "final" ]]; then
    echo "gc_capacity_final: owner final arm completed; record the JSON and host class in the capacity model."
fi
