SELECT 1 AS ok, pg_backend_pid()::bigint AS pid WHERE pg_try_advisory_lock($1::bigint);
