set -e
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check && echo "fmt=clean"
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-storage --example measure_ingest 2>&1 | tail -1
CAMP=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-ingest-20260920T232000Z
mkdir -p $CAMP/runs
EXE=core/target/release/examples/measure_ingest
for p in noise repeat; do rm -rf $CAMP/runs/ingest-500m-$p; $EXE --bytes 524288000 --output $CAMP/runs/ingest-500m-$p --pattern $p 2>&1 | tee $CAMP/runs/ingest-500m-$p.txt; echo; done
shasum -a 256 $CAMP/runs/ingest-500m-noise/sample.sqlite $CAMP/runs/ingest-500m-repeat/sample.sqlite
cp core/crates/layerfs-storage/examples/measure_ingest.rs $CAMP/measure_ingest.rs.txt
echo INGEST_DONE
