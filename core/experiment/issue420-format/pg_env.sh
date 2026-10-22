# Issue #420 format study: own bounded PG instance (never lfs420-pg, lfs420-bundle-pg or lfs420-size-pg).
C=lfs420-format-pg
q() { docker exec -i $C psql -X -q -v ON_ERROR_STOP=1 -U postgres -d "${DB:-fmt}" "$@"; }
