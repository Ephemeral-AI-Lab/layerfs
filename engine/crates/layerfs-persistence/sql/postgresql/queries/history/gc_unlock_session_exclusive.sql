SELECT pg_advisory_unlock($1::bigint) AS ok, pg_backend_pid()::bigint AS pid;
