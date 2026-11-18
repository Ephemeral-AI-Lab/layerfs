SELECT a.pid::bigint,
       a.client_addr::text,
       a.backend_start::text
  FROM pg_locks l
  JOIN pg_stat_activity a ON a.pid = l.pid
 WHERE l.locktype = 'advisory'
   AND l.granted
   AND l.classid::bigint = (($1::bigint >> 32) & 4294967295)
   AND l.objid::bigint = ($1::bigint & 4294967295)
   AND l.objsubid = 1
   AND a.pid <> pg_backend_pid()
 ORDER BY a.backend_start
 LIMIT 1;
