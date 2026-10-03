SELECT tablename::text FROM pg_catalog.pg_tables WHERE schemaname = $1 ORDER BY tablename;
