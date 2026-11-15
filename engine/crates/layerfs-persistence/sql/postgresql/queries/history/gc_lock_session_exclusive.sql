SELECT 1 AS ok WHERE pg_try_advisory_lock($1::bigint);
