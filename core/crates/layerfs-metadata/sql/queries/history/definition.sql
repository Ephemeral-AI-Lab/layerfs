SELECT string_agg(part, E'\n' ORDER BY part) FROM (
 SELECT 'column:' || c.relname || ':' || a.attname || ':' || pg_catalog.format_type(a.atttypid,a.atttypmod) || ':' || a.attnotnull || ':' || a.attnum AS part FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid = a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 AND c.relname = ANY($2) AND a.attnum > 0 AND NOT a.attisdropped
 UNION ALL SELECT 'constraint:' || c.relname || ':' || pg_catalog.pg_get_constraintdef(k.oid,true) FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=ANY($2)
 UNION ALL SELECT 'index:' || tablename || ':' || indexdef FROM pg_catalog.pg_indexes WHERE schemaname=$1 AND tablename=ANY($2)
 UNION ALL SELECT 'trigger:' || c.relname || ':' || pg_catalog.pg_get_triggerdef(t.oid,true) FROM pg_catalog.pg_trigger t JOIN pg_catalog.pg_class c ON c.oid=t.tgrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=ANY($2) AND NOT t.tgisinternal
) definitions;
