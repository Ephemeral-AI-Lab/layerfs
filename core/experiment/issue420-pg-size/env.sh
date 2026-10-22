# Issue #420 size study: own PG instance, never the contention test's lfs420-pg.
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/.claude/worktrees/issue420-postgresql
TS=${TS:-20261022T191100Z}
OUT=$WT/core/target/issue420/size/$TS
C=lfs420-size-pg
NET=lfs420-size-net
VOL=lfs420-size-data
FIX=$WT/core/target/issue420/fixture
# psql inside the container (unix socket), database $DB (default size)
q() { docker exec -i $C psql -X -v ON_ERROR_STOP=1 -U postgres -d "${DB:-size}" "$@"; }
# the copied port binary, on the container's network namespace, fixture read-only
bin() { docker run --rm --network container:$C -v $FIX:/fix:ro -v $OUT/bin:/b:ro postgres:17-bookworm /b/issue420-pg "$@"; }
