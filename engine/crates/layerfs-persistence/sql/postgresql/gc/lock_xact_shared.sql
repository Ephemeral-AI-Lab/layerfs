SELECT 1 AS ok WHERE pg_try_advisory_xact_lock_shared($1::bigint);
