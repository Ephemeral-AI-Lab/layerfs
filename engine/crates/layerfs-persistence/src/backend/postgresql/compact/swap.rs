//! §8 Phase B: the pack swap, its tuple re-read and its verification.

use super::build::relation_bytes;
use super::build::relation_bytes_tx;
use super::cleanup::drop_scratch;
use super::op::hex;
use super::op::quote_ident;
use super::op::scalar;
use super::op::stage;
use super::sync::effective_sync_rank;
use super::sync::standby_check;
use super::sync::sync_name;
use super::types::Built;
use super::types::DeadTotals;
use super::types::Swapped;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::postgresql::session::transaction::Transaction;
use crate::backend::records::Param;
use crate::diagnostic::postgresql_contract::check_table_objects;
use crate::history::GcTuple;
use crate::MarkResult;
use crate::PostgresHandles;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use layerfs_history::error::OpConflict;
use layerfs_history::records::GcIntentKind;
use layerfs_history::records::GcScopeKind;
use std::sync::Arc;

/// Phase B: check the scratch tables' rows against the mark, then lock every
/// base table, re-check the mark tuple, swap the two payload tables, bump the
/// generation and drop the scratch schema, all in one transaction. The row
/// checks scan the scratch tables, so they run before the Store-wide lock
/// and no reader or writer waits for them. `55P03` is `Busy`; any tuple
/// change is `Conflict` with no writes.
pub(crate) fn swap(
    _handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    tuple: &GcTuple,
    mark: &MarkResult,
    built: &Built,
    op_id: &[u8; 16],
) -> HistoryResult<Swapped> {
    let physical_before = relation_bytes(session, schema, &["pack", "object_location"])?;
    let rank = effective_sync_rank(session)?;
    let tables = tables_of(session, schema)?;
    let locked = tables
        .iter()
        .map(|table| format!("{schema}.{}", quote_ident(table)))
        .collect::<Vec<_>>()
        .join(", ");
    let scratch = built.scratch.clone();
    let result = stage(session, |tx| -> HistoryResult<Swapped> {
        tx.batch(&format!(
            "SET LOCAL synchronous_commit TO {}",
            sync_name(rank)
        ))?;
        verify_scratch(tx, &scratch, mark, tuple, &built.dead)?;
        tx.batch(&format!(
            "LOCK TABLE {locked} IN ACCESS EXCLUSIVE MODE NOWAIT"
        ))?;
        // The tuple the mark recorded, re-read under the lock. Any publication
        // after the mark moved one of these, so nothing is written.
        let now = read_tuple(tx, schema)?;
        if now != *tuple {
            return Err(HistoryError::Conflict("Store moved after the mark"));
        }
        // Read once: the lock is held from here to COMMIT, so the heads this
        // digest covers cannot move again inside this transaction. The mark's
        // own statements read them, a bounded page at a time.
        if crate::history::live_heads_digest_in(tx)? != mark.live_heads_digest() {
            return Err(HistoryError::Conflict("live heads moved after the mark"));
        }
        // The ACL lives on the tables this transaction drops, so read it
        // first; the owner and both ACL levels are replayed on the NEW tables
        // after `SET SCHEMA`, when they are the schema's tables again. A
        // `GRANT` before the drop would name the table that is about to go.
        let acls = [
            ("pack", read_acl(tx, schema, "pack")?),
            ("object_location", read_acl(tx, schema, "object_location")?),
        ];
        tx.batch(&format!(
            "DROP TABLE {schema}.object_location;\
             DROP TABLE {schema}.pack;\
             ALTER TABLE {scratch}.object_location SET SCHEMA {schema};\
             ALTER TABLE {scratch}.pack SET SCHEMA {schema};\
             CREATE TRIGGER pack_immutable BEFORE UPDATE OR DELETE ON {schema}.pack \
              FOR EACH ROW EXECUTE FUNCTION {schema}.pack_immutable();"
        ))?;
        // Owner, PUBLIC grants, grant options and column ACLs, on the tables
        // that survive.
        for (table, acl) in &acls {
            apply_acl(tx, schema, table, acl)?;
        }
        // The mark recorded both source tables' pg_attribute/attstorage/
        // reloptions/tablespace fingerprint; the moved scratch tables must
        // carry it exactly, or the rebuild silently changed their layout.
        let op = tx.query(
            &format!(
                "SELECT COALESCE(mark_fingerprint,''),totals_applied FROM {schema}.gc_op WHERE id=1"
            ),
            Vec::new(),
        )?;
        let op = op
            .first()
            .ok_or(HistoryError::Integrity("compaction fingerprint row"))?;
        let recorded: String = op.get(0)?;
        let applied: i64 = op.get(1)?;
        let moved = format!(
            "{}|{}",
            super::op::table_fingerprint(tx, schema, "pack")?,
            super::op::table_fingerprint(tx, schema, "object_location")?
        );
        if recorded != moved {
            return Err(HistoryError::Integrity("compaction table fingerprint"));
        }
        // `store_totals.logical` equals the mark's under the lock (the tuple
        // re-check above), so the history rows are summed against that value.
        let logical = scalar(
            tx,
            &format!(
                "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.layer)"
            ),
            Vec::new(),
        )?;
        if logical != tuple.logical {
            return Err(HistoryError::Integrity(
                "Store totals disagree with the history rows",
            ));
        }
        // The trigger and immutable-pack function entries need the
        // Store-schema function, so they are verified here, bounded to the
        // moved table, after the trigger exists.
        tx.batch(&format!("SET LOCAL search_path TO {schema}, pg_catalog"))?;
        let failures =
            check_table_objects(tx, &["pack"], &["T", "F"]).map_err(HistoryError::from)?;
        if !failures.is_empty() {
            return Err(HistoryError::Integrity("compaction moved-table contract"));
        }
        if applied != 0 {
            return Err(HistoryError::Integrity("compaction totals already applied"));
        }
        // §11: the swap's own post-swap relation size, measured inside the
        // transaction that makes the moved tables live. Persisting it keeps a
        // save that lands after the swap (AC-28) out of a resumed `G + 1`
        // report's `physicalShrink`.
        let physical_after = relation_bytes_tx(tx, schema, &["pack", "object_location"])?;
        // §11: the swap records everything a resumed `G + 1` report cannot
        // recompute after the old tables are gone: both relation sizes, the
        // Phase-A peak and the mark's kept waste.
        tx.query(
            &format!(
                "UPDATE {schema}.gc_op SET phase='swap',dead_canonical=$1,dead_stored=$2,dead_metadata=$3,physical_before=$4,physical_after=$5,peak_temp=$6 WHERE id=1"
            ),
            vec![
                Param::I64(built.dead.canonical),
                Param::I64(built.dead.stored),
                Param::I64(built.dead.metadata),
                Param::I64(i64::try_from(physical_before).unwrap_or(i64::MAX)),
                Param::I64(i64::try_from(physical_after).unwrap_or(i64::MAX)),
                Param::I64(i64::try_from(built.peak_temp).unwrap_or(i64::MAX)),
            ],
        )?;
        tx.query(
            &format!(
                "UPDATE {schema}.history_meta SET catalog_incarnation={} WHERE id=1",
                tuple.generation + 1
            ),
            Vec::new(),
        )?;
        tx.batch(&format!("DROP SCHEMA {scratch} CASCADE"))?;
        let names: String = tx
            .query(
                "SELECT current_setting('synchronous_standby_names')",
                Vec::new(),
            )?
            .first()
            .ok_or(HistoryError::Integrity("standby names"))?
            .get(0)?;
        standby_check(tx, rank, &names, op_id, "swap")?;
        Ok(Swapped {
            physical_before,
            physical_after,
            peak_temp: built.peak_temp,
            dead: built.dead,
        })
    });
    let result = match result {
        Ok(swapped) => swapped,
        Err(HistoryError::Busy) => {
            // §8 step 5: roll back first, then drop the scratch schema in a
            // new transaction, and report Busy with the holder detail (the
            // backend holding the base table's ACCESS EXCLUSIVE lock).
            let _ = drop_scratch(session, &scratch);
            return Err(swap_busy(session, schema, op_id)?);
        }
        Err(other) => return Err(other),
    };
    // The swap committed at `G + 1`: this session's own fence must follow it,
    // or its next write transaction would (correctly) see a stale generation.
    session.incarnation.store(
        u64::try_from(tuple.generation.saturating_add(1)).unwrap_or(0),
        std::sync::atomic::Ordering::SeqCst,
    );
    Ok(result)
}

/// The `Busy` a contended swap reports: the compaction identity with the
/// backend holding any granted table lock on the Store's base tables, in any
/// mode, read from `pg_locks`/`pg_stat_activity`. A reader's
/// `AccessShareLock` conflicts with the swap's `ACCESS EXCLUSIVE` lock just
/// as another writer's does, so filtering on `AccessExclusiveLock` alone
/// turned a reader-blocked swap into a holder-less `Busy`.
pub(crate) fn swap_busy(
    session: &Arc<Session>,
    schema: &str,
    op_id: &[u8; 16],
) -> HistoryResult<HistoryError> {
    let mut conflict = OpConflict::new(
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        "store",
        Some("swap".to_owned()),
        hex(op_id),
        false,
    );
    let holder = stage(session, |tx| {
        let rows = tx.query(
            // 55P03 does not name the blocking mode: an ordinary reader's
            // granted AccessShareLock blocks the swap exactly as another
            // writer does, so any granted table lock by another backend is
            // the holder here.
            "SELECT a.pid::bigint, a.client_addr::text, a.backend_start::text \
               FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid \
               JOIN pg_class c ON c.oid=l.relation \
               JOIN pg_namespace n ON n.oid=c.relnamespace \
              WHERE n.nspname=$1 AND c.relkind='r' \
                AND l.granted AND a.pid <> pg_backend_pid() \
              ORDER BY a.backend_start LIMIT 1",
            vec![Param::Text(schema.to_owned())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(Some((
            row.get::<i64>(0)?,
            row.get::<Option<String>>(1)?,
            row.get::<Option<String>>(2)?,
        )))
    })?;
    if let Some((pid, client_addr, backend_start)) = holder {
        conflict = conflict.with_backend(Some(pid), client_addr, backend_start);
    }
    Ok(HistoryError::BusyOp(Box::new(conflict)))
}

pub(crate) fn tables_of(session: &Arc<Session>, schema: &str) -> HistoryResult<Vec<String>> {
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname=$1 AND c.relkind='r' ORDER BY c.relname",
            vec![Param::Text(schema.to_owned())],
        )?;
        rows.iter()
            .map(|row| row.get::<String>(0).map_err(HistoryError::from))
            .collect()
    })
}

pub(crate) fn read_tuple(tx: &Transaction<'_>, schema: &str) -> HistoryResult<GcTuple> {
    let rows = tx.query(
        &format!(
            "SELECT h.catalog_incarnation,h.next_history_serial,p.next_pack_id,p.next_ordinal,\
             t.logical,t.canonical,t.stored,t.metadata,\
             (SELECT COALESCE(MAX(stamp),0) FROM {schema}.content_signature),\
             (SELECT COUNT(*) FROM {schema}.\"commit\")+(SELECT COUNT(*) FROM {schema}.layer) \
             FROM {schema}.history_meta h, {schema}.store_policy p, {schema}.store_totals t \
             WHERE h.id=1 AND p.id=1 AND t.id=1"
        ),
        Vec::new(),
    )?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("Store tuple"));
    }
    let row = &rows[0];
    Ok(GcTuple {
        generation: row.get(0)?,
        next_history_serial: row.get(1)?,
        next_pack_id: row.get(2)?,
        next_ordinal: row.get(3)?,
        logical: row.get(4)?,
        canonical: row.get(5)?,
        stored: row.get(6)?,
        metadata: row.get(7)?,
        stamp: row.get(8)?,
        history_rows: row.get(9)?,
    })
}

/// The owner, table ACL and column ACLs of one payload table, read before the
/// swap drops the table that holds them.
pub(crate) struct TableAcl {
    pub(crate) owner: String,
    pub(crate) table: Vec<AclGrant>,
    pub(crate) columns: Vec<(String, Vec<AclGrant>)>,
}

/// One parsed `aclitem`: the grantee (`PUBLIC` for the empty grantee), the
/// privilege names and whether they carry the grant option.
pub(crate) struct AclGrant {
    pub(crate) grantee: String,
    pub(crate) privileges: Vec<&'static str>,
    pub(crate) grant_option: bool,
}

/// The privilege a PostgreSQL ACL letter names; `MAINTAIN` exists from 17.
pub(crate) fn privilege_name(letter: char) -> Option<&'static str> {
    Some(match letter {
        'r' => "SELECT",
        'w' => "UPDATE",
        'a' => "INSERT",
        'd' => "DELETE",
        'D' => "TRUNCATE",
        'x' => "REFERENCES",
        't' => "TRIGGER",
        'm' => "MAINTAIN",
        _ => return None,
    })
}

/// Parses newline-separated `aclitem`s, dropping the owner's own implicit
/// entry (recreated by `ALTER TABLE ... OWNER TO`) and splitting a privilege
/// list into its plain and grant-option parts.
pub(crate) fn acl_items(acl: &str) -> Vec<AclGrant> {
    let mut grants = Vec::new();
    for line in acl.lines().filter(|line| !line.is_empty()) {
        let Some((grantee, rest)) = line.split_once('=') else {
            continue;
        };
        let Some((letters, grantor)) = rest.split_once('/') else {
            continue;
        };
        if grantee == grantor {
            continue;
        }
        let mut plain = Vec::new();
        let mut optioned = Vec::new();
        let chars: Vec<char> = letters.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            let letter = chars[index];
            let option = chars.get(index + 1) == Some(&'*');
            if let Some(name) = privilege_name(letter) {
                if option {
                    optioned.push(name);
                } else {
                    plain.push(name);
                }
            }
            index += if option { 2 } else { 1 };
        }
        let grantee = if grantee.is_empty() {
            "PUBLIC".to_owned()
        } else {
            quote_ident(grantee)
        };
        if !plain.is_empty() {
            grants.push(AclGrant {
                grantee: grantee.clone(),
                privileges: plain,
                grant_option: false,
            });
        }
        if !optioned.is_empty() {
            grants.push(AclGrant {
                grantee,
                privileges: optioned,
                grant_option: true,
            });
        }
    }
    grants
}

/// Reads one payload table's owner and both ACL levels.
pub(crate) fn read_acl(tx: &Transaction<'_>, schema: &str, table: &str) -> HistoryResult<TableAcl> {
    let rows = tx.query(
        "SELECT pg_get_userbyid(c.relowner)::text, COALESCE(array_to_string(c.relacl,E'\\n'),'') \
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname=$1 AND c.relname=$2",
        vec![
            Param::Text(schema.to_owned()),
            Param::Text(table.to_owned()),
        ],
    )?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("compaction owner"));
    }
    let owner: String = rows[0].get(0)?;
    let table_acl: String = rows[0].get(1)?;
    let column_rows = tx.query(
        "SELECT a.attname::text, COALESCE(array_to_string(a.attacl,E'\\n'),'') \
         FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid \
         JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname=$1 AND c.relname=$2 AND a.attacl IS NOT NULL AND a.attnum>0 \
         ORDER BY a.attnum",
        vec![
            Param::Text(schema.to_owned()),
            Param::Text(table.to_owned()),
        ],
    )?;
    let mut columns = Vec::new();
    for row in &column_rows {
        let name: String = row.get(0)?;
        let acl: String = row.get(1)?;
        columns.push((name, acl_items(&acl)));
    }
    Ok(TableAcl {
        owner,
        table: acl_items(&table_acl),
        columns,
    })
}

/// One `GRANT` statement for an `aclitem`. A column ACL names the column after
/// each privilege (`GRANT SELECT (c), UPDATE (c) ...`); the table ACL has no
/// column list.
pub(crate) fn grant_statement(
    schema: &str,
    table: &str,
    grant: &AclGrant,
    column: Option<&str>,
) -> String {
    let privileges = match column {
        Some(column) => grant
            .privileges
            .iter()
            .map(|name| format!("{name} ({})", quote_ident(column)))
            .collect::<Vec<_>>()
            .join(", "),
        None => grant.privileges.join(", "),
    };
    format!(
        "GRANT {privileges} ON TABLE {schema}.{} TO {}{}",
        quote_ident(table),
        grant.grantee,
        if grant.grant_option {
            " WITH GRANT OPTION"
        } else {
            ""
        }
    )
}

/// Replays one table's owner and ACLs on the table that is now in `schema`.
pub(crate) fn apply_acl(
    tx: &Transaction<'_>,
    schema: &str,
    table: &str,
    acl: &TableAcl,
) -> HistoryResult<()> {
    tx.batch(&format!(
        "ALTER TABLE {schema}.{} OWNER TO {}",
        quote_ident(table),
        quote_ident(&acl.owner)
    ))?;
    for grant in &acl.table {
        tx.batch(&grant_statement(schema, table, grant, None))?;
    }
    for (column, grants) in &acl.columns {
        for grant in grants {
            tx.batch(&grant_statement(schema, table, grant, Some(column)))?;
        }
    }
    Ok(())
}

/// §8 step 9's row counts and sums, read from the scratch tables inside the
/// swap transaction and before the Store-wide lock. The two tables are locked
/// first, so the rows counted here are the rows `SET SCHEMA` moves; the lock
/// waits (no `NOWAIT`) because only maintenance on the scratch schema can
/// hold it, and nothing of the Store is locked yet. Both sides of each
/// comparison come from the mark, which the tuple re-check then pins.
pub(crate) fn verify_scratch(
    tx: &Transaction<'_>,
    scratch: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
    dead: &DeadTotals,
) -> HistoryResult<()> {
    tx.batch(&format!(
        "LOCK TABLE {scratch}.pack, {scratch}.object_location IN ACCESS EXCLUSIVE MODE"
    ))?;
    let rows = tx.query(
        &format!(
            "SELECT o.objects,o.distinct_objects,o.canonical,p.packs,p.stored,p.metadata FROM \
             (SELECT count(*) AS objects,count(DISTINCT object_id) AS distinct_objects,\
                     COALESCE(SUM(canonical_length)::bigint,0) AS canonical \
                FROM {scratch}.object_location) o,\
             (SELECT count(*) AS packs,COALESCE(SUM(length)::bigint,0) AS stored,\
                     COALESCE(SUM(length) FILTER (WHERE domain=0)::bigint,0) AS metadata \
                FROM {scratch}.pack) p"
        ),
        Vec::new(),
    )?;
    let row = rows
        .first()
        .ok_or(HistoryError::Integrity("compaction scratch counts"))?;
    let objects: i64 = row.get(0)?;
    let distinct: i64 = row.get(1)?;
    let canonical: i64 = row.get(2)?;
    let packs: i64 = row.get(3)?;
    let stored: i64 = row.get(4)?;
    let metadata: i64 = row.get(5)?;
    if objects != mark.objects().len() as i64
        || distinct != objects
        || packs != mark.objects().retained_packs().len() as i64
    {
        return Err(HistoryError::Integrity("compaction swapped count"));
    }
    if canonical != tuple.canonical - dead.canonical
        || stored != tuple.stored - dead.stored
        || metadata != tuple.metadata - dead.metadata
    {
        return Err(HistoryError::Integrity("compaction swapped totals"));
    }
    Ok(())
}
