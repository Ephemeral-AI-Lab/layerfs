-- Gives back one level of each of $1 (shared where $2 says so) in one
-- statement, with the backend that ran it (spec §5.2 item 34). It reads no
-- table and runs whatever the Store's generation: keys are only given back.
SELECT CASE WHEN k.shared THEN pg_advisory_unlock_shared(k.key)
            ELSE pg_advisory_unlock(k.key) END AS released,
       pg_backend_pid()::bigint AS pid
  FROM unnest($1::bigint[], $2::boolean[]) AS k(key, shared);
