SELECT 1 AS ok WHERE pg_try_advisory_lock_shared($1::bigint);
