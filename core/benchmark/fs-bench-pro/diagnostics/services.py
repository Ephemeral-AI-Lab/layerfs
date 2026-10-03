"""Explicit diagnostic-only PG observation profile; durability and limits unchanged."""
import json
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from shared.phase7 import services


def configure():
    services.postgres_sql("ALTER SYSTEM SET shared_preload_libraries='pg_stat_statements';")
    services.docker('restart',services.NAMES['postgres'])
    settings=services.load();services.wait_ready(settings);services.validate(settings)
    services.postgres_sql("CREATE EXTENSION pg_stat_statements; ALTER SYSTEM SET pg_stat_statements.track='all'; ALTER SYSTEM SET pg_stat_statements.track_planning='on'; ALTER SYSTEM SET track_io_timing='on'; ALTER SYSTEM SET track_wal_io_timing='on'; SELECT pg_reload_conf();")
    rows=services.postgres_sql("SELECT name||'='||setting FROM pg_settings WHERE name IN ('shared_preload_libraries','pg_stat_statements.track','pg_stat_statements.track_planning','pg_stat_statements.track_utility','pg_stat_statements.max','compute_query_id','track_io_timing','track_wal_io_timing','fsync','synchronous_commit','full_page_writes') ORDER BY name;")
    if not all(value in rows.splitlines() for value in ('fsync=on','synchronous_commit=on','full_page_writes=on','pg_stat_statements.track=all','pg_stat_statements.track_planning=on')):
        raise ValueError('diagnostic PG observation profile did not apply')
    return {'scope':'CAUSE_DIAGNOSTIC; added observer extension/settings, no durability or resource relaxation','settings':rows.splitlines()}


def snapshot():
    query="""SELECT json_build_object('database',(SELECT row_to_json(s) FROM pg_stat_database s WHERE datname=current_database()),'wal',(SELECT row_to_json(s) FROM pg_stat_wal s),'io',(SELECT json_agg(s) FROM pg_stat_io s),'statement_info',(SELECT row_to_json(s) FROM pg_stat_statements_info s));"""
    return json.loads(services.postgres_sql(query))


def statements():
    return json.loads(services.postgres_sql("SELECT COALESCE(json_agg(s),'[]') FROM (SELECT queryid::text,query,toplevel,plans,calls,total_plan_time,total_exec_time,rows,shared_blks_hit,shared_blks_read,shared_blks_dirtied,shared_blks_written,temp_blks_read,temp_blks_written,shared_blk_read_time,shared_blk_write_time,wal_records,wal_fpi,wal_bytes::text FROM pg_stat_statements WHERE userid=(SELECT oid FROM pg_roles WHERE rolname='layerfs') ORDER BY toplevel DESC,queryid) s;"))
