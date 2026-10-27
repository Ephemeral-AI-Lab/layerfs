//! Fixed statement templates shared by ordinary jobs and plan diagnostics.
/// The latest local row of one serial, with the generation that holds it and
/// its payload-layer columns: a job that publishes the serial reads no layer
/// row again.
pub(crate) const INODE_LOOKUP: &str =
    "SELECT inode,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs,generation,epoch,height,referenced,sequence
    FROM fs_inode WHERE inode=?1 AND generation<=?2 AND generation>?3 ORDER BY generation DESC LIMIT 1";
pub(crate) const INODE_CAPTURE: &str =
    "SELECT inode,kind,mode,mtime_seconds,mtime_nanoseconds,nlink,size,inherited_cutoff,born,entries,subdirs
    FROM fs_inode INDEXED BY fs_inode_capture WHERE generation=?1 AND inode>?2 ORDER BY inode LIMIT 64";
/// The last row at or before cell ?4 in its slot, which starts at ?3. A row
/// of one cell at ?4 returns its bytes and mask; any other row only its
/// shape, so a wide row is never loaded to learn that it is there.
pub(crate) const CELL_COVER: &str = "SELECT rowid,cell_offset,epoch,length(data),
    CASE WHEN cell_offset=?4 AND length(data)<=4096 THEN data END,
    CASE WHEN cell_offset=?4 THEN validity END FROM fs_data
    WHERE inode=?1 AND generation=?2 AND cell_offset>=?3 AND cell_offset<=?4
    ORDER BY cell_offset DESC LIMIT 1";
/// Shapes of the rows whose first cell lies in [?3, ?4); no bytes are loaded.
pub(crate) const CELL_SHAPES: &str =
    "SELECT rowid,cell_offset,epoch,length(data),length(validity) FROM fs_data
    WHERE inode=?1 AND generation=?2 AND cell_offset>=?3 AND cell_offset<?4
    ORDER BY cell_offset";
pub(crate) const CELL_PUT: &str =
    "INSERT INTO fs_data(inode,generation,cell_offset,epoch,data,validity) VALUES(?1,?2,?3,?4,?5,?6)
    ON CONFLICT(inode,generation,cell_offset) DO UPDATE SET epoch=excluded.epoch,
    data=excluded.data,validity=excluded.validity";
pub(crate) const CELL_DROP: &str =
    "DELETE FROM fs_data WHERE inode=?1 AND generation=?2 AND cell_offset=?3";
pub(crate) const CELLS_DROP: &str = "DELETE FROM fs_data
    WHERE inode=?1 AND generation=?2 AND cell_offset>=?3 AND cell_offset<?4";
pub(crate) const ROW_DROP: &str = "DELETE FROM fs_data WHERE rowid=?1";
/// Cuts a dense row to its first ?2 bytes.
pub(crate) const ROW_KEEP: &str = "UPDATE fs_data SET data=substr(data,1,?2) WHERE rowid=?1";
pub(crate) const ROW_MOVE: &str = "UPDATE fs_data SET generation=?2,epoch=?3 WHERE rowid=?1";
/// ?3 bytes of a row from its byte ?2, counted from one.
pub(crate) const ROW_SLICE: &str = "SELECT substr(data,?2,?3) FROM fs_data WHERE rowid=?1";
/// Rows that hold a byte of [?5, ?4): a row can start as early as its slot,
/// ?3, and one that ends at or before ?5 is passed over without its bytes.
pub(crate) const CELL_RANGE: &str = "SELECT cell_offset,epoch,data,validity FROM fs_data
    WHERE inode=?1 AND generation=?2 AND cell_offset>=?3 AND cell_offset<?4
    AND cell_offset+length(data)>?5 ORDER BY cell_offset";
/// The next row at or after ?3 that reaches past cell ?5.
pub(crate) const CAPTURED_CELL_METADATA: &str =
    "SELECT cell_offset,epoch,length(data),length(validity) FROM fs_data
    WHERE inode=?1 AND generation=?2 AND cell_offset>=?3 AND cell_offset<?4
    AND cell_offset+length(data)>?5 ORDER BY cell_offset LIMIT 1";
/// The last piece of one layer that starts at or before byte ?3.
pub(crate) const PIECE_COVER: &str =
    "SELECT file_offset,length,epoch,source,source_offset FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset<=?3 ORDER BY file_offset DESC LIMIT 1";
/// The pieces of one layer that can hold a byte of [?3, ?4): the last one
/// that starts at or before ?3 and those that start inside. The LIMIT is
/// `pieces::rows::WINDOW_ROWS`, kept equal by hand: a bound limit would make
/// SQLite prepare the statement again whenever its value is bound.
pub(crate) const PIECE_WINDOW: &str =
    "SELECT file_offset,length,epoch,source,source_offset FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset>=ifnull((SELECT max(file_offset)
    FROM fs_data_ref WHERE generation=?1 AND inode=?2 AND file_offset<=?3),?3)
    AND file_offset<?4 ORDER BY file_offset LIMIT 34";
pub(crate) const PIECE_PUT: &str = "INSERT INTO fs_data_ref VALUES(?1,?2,?3,?4,?5,?6,?7)";
pub(crate) const PIECE_LENGTH: &str = "UPDATE fs_data_ref SET length=?4
    WHERE generation=?1 AND inode=?2 AND file_offset=?3";
/// Removes the pieces of one layer that start in [?3, ?4) and returns them.
pub(crate) const PIECES_TAKE: &str = "DELETE FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset>=?3 AND file_offset<?4
    RETURNING file_offset,length,epoch,source,source_offset";
/// Ends the piece that crosses byte ?3 of one layer at that byte.
pub(crate) const PIECE_CUT: &str = "UPDATE fs_data_ref SET length=?3-file_offset
    WHERE generation=?1 AND inode=?2 AND file_offset=(SELECT max(file_offset)
    FROM fs_data_ref WHERE generation=?1 AND inode=?2 AND file_offset<?3)
    AND file_offset+length>?3";
/// The first piece of one layer that starts at or after byte ?3.
pub(crate) const PIECE_NEXT: &str =
    "SELECT file_offset,length,epoch,source,source_offset FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset>=?3 ORDER BY file_offset LIMIT 1";
/// Moves the start of one piece forward: its new first byte, length and
/// source offset.
pub(crate) const PIECE_START: &str =
    "UPDATE fs_data_ref SET file_offset=?4,length=?5,source_offset=?6
    WHERE generation=?1 AND inode=?2 AND file_offset=?3";
pub(crate) const PIECE_DROP: &str = "DELETE FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset=?3";
/// The lowest `?3 + 1` pieces of one layer, found by one seek and that many
/// index steps.
pub(crate) const PIECES_DROP: &str = "DELETE FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset<=ifnull((SELECT file_offset
    FROM fs_data_ref WHERE generation=?1 AND inode=?2
    ORDER BY file_offset LIMIT 1 OFFSET ?3),9223372036854775807)";
/// One page of the pieces of one layer after byte ?3, with their stamps.
pub(crate) const PIECE_PAGE: &str = "SELECT file_offset,epoch FROM fs_data_ref
    WHERE generation=?1 AND inode=?2 AND file_offset>?3 ORDER BY file_offset LIMIT 64";
/// One page of the pieces of one generation, in key order.
pub(crate) const PIECE_GENERATION_PAGE: &str = "SELECT inode,file_offset FROM fs_data_ref
    WHERE generation=?1 AND (inode,file_offset)>(?2,?3) ORDER BY inode,file_offset LIMIT 64";
pub(crate) const LAYER_REFERENCED: &str =
    "UPDATE fs_inode SET referenced=1 WHERE inode=?1 AND generation=?2";
pub(crate) const STEP_GET: &str =
    "SELECT cell_offset,epoch FROM fs_truncate WHERE inode=?1 AND generation=?2 AND depth=?3";
pub(crate) const STEP_PUT: &str = "INSERT INTO fs_truncate VALUES(?1,?2,?3,?4,?5)
    ON CONFLICT(inode,generation,depth) DO UPDATE SET cell_offset=excluded.cell_offset,
    epoch=excluded.epoch";
pub(crate) const LAYERS: &str =
    "SELECT generation,kind,size,inherited_cutoff,epoch,height,nlink,referenced,sequence FROM fs_inode
    WHERE inode=?1 AND generation<=?2 AND generation>?3 ORDER BY generation DESC LIMIT 4";
pub(crate) const LAYER_ACTIVE: &str =
    "SELECT size,inherited_cutoff,epoch,height,referenced,sequence
    FROM fs_inode
    WHERE inode=?1 AND generation=?2";
pub(crate) const LAYER_LOWER: &str = "SELECT size FROM fs_inode
    WHERE inode=?1 AND generation<?2 AND generation>?3 ORDER BY generation DESC LIMIT 1";
/// The active row and the latest lower row of one name in one seek: the row
/// whose generation is the upper bound is active, the first below it is lower.
pub(crate) const NAME_LAYERS: &str = "SELECT generation,inode,inherited FROM fs_entry
    WHERE parent=?1 AND name=?2 AND generation<=?3 AND generation>?4 ORDER BY generation DESC LIMIT 2";
pub(crate) const DIRECTORY_ENTRY_LOOKUP:&str="SELECT inode,inherited FROM fs_entry WHERE parent=?1 AND name=?2 AND generation<=?3 AND generation>?4 ORDER BY generation DESC LIMIT 1";
pub(crate) const DIRECTORY_ENTRY_CAPTURE: &str = "SELECT parent,name,inode,inherited
    FROM fs_entry INDEXED BY fs_entry_capture WHERE generation=?1
    AND (parent,name)>(?2,?3) ORDER BY parent,name LIMIT 64";
pub(crate) const SOURCE_NAMES: &str =
    "SELECT parent,name,inode,inherited FROM fs_entry INDEXED BY fs_entry_capture
    WHERE generation=?1 AND parent=?2 AND name>?3 ORDER BY name LIMIT 64";
/// The same window with the kind of each bound name's latest local inode
/// row at the current view, or NULL when the inode has none: one seek per
/// returned name inside this one statement.
pub(crate) const SOURCE_NAMES_KINDS: &str =
    "SELECT e.parent,e.name,e.inode,e.inherited,(SELECT i.kind FROM fs_inode i
    WHERE i.inode=e.inode AND i.generation<=?4 AND i.generation>?5 ORDER BY i.generation DESC LIMIT 1)
    FROM fs_entry e INDEXED BY fs_entry_capture
    WHERE e.generation=?1 AND e.parent=?2 AND e.name>?3 ORDER BY e.name LIMIT 64";
pub(crate) const CAPTURED_DIRECTORY_ENTRY: &str =
    "SELECT inode,inherited FROM fs_entry WHERE parent=?1 AND name=?2 AND generation=?3";
/// The first row of a serial in a generation. A row that already exists is a
/// constraint failure, never an update. A new layer holds no reference piece.
pub(crate) const INODE_INSERT: &str = "INSERT INTO fs_inode
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,?15)";
/// Every value of a row that exists, with the parameters of the insert. The
/// reference flag is not a value of the job: only a published piece sets it.
pub(crate) const INODE_UPDATE: &str = "UPDATE fs_inode SET kind=?3,mode=?4,
    mtime_seconds=?5,mtime_nanoseconds=?6,nlink=?7,size=?8,inherited_cutoff=?9,
    born=?10,entries=?11,subdirs=?12,epoch=?13,height=?14,sequence=?15
    WHERE inode=?1 AND generation=?2";
pub(crate) const DIRECTORY_ENTRY_ACTIVE: &str =
    "SELECT inode,inherited FROM fs_entry WHERE parent=?1 AND name=?2 AND generation=?3";
pub(crate) const DIRECTORY_ENTRY_PUT: &str = "INSERT INTO fs_entry VALUES(?1,?2,?3,?4,?5)
    ON CONFLICT(parent,name,generation) DO UPDATE SET inode=excluded.inode,inherited=excluded.inherited";
pub(crate) const DIRECTORY_ENTRY_DROP: &str =
    "DELETE FROM fs_entry WHERE parent=?1 AND name=?2 AND generation=?3";

pub(crate) const INDEXED_OPERATION_RECORD_CONTAINS: &str = "SELECT 1 FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key=?4";
pub(crate) const INDEXED_OPERATION_RECORD_GET: &str = "SELECT value FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key=?4";
pub(crate) const INDEXED_OPERATION_RECORD_PUT: &str =
    "INSERT INTO commit_record VALUES(?1,?2,?3,?4,?5)
    ON CONFLICT(operation,file_scope,kind,key) DO UPDATE SET value=excluded.value";
pub(crate) const INDEXED_OPERATION_RECORD_DELETE: &str = "DELETE FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key=?4";
pub(crate) const INDEXED_OPERATION_RECORD_KEYS: &str = "SELECT key FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key<>?4
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_ALL_KEYS: &str = "SELECT key FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_KEYS_AFTER: &str = "SELECT key FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key>?4
    ORDER BY key LIMIT 64";
/// Keys with their values in key order: one window of a sealed sequence.
pub(crate) const INDEXED_OPERATION_RECORD_ENTRIES: &str = "SELECT key,value FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3
    ORDER BY key LIMIT 64";
pub(crate) const INDEXED_OPERATION_RECORD_ENTRIES_AFTER: &str =
    "SELECT key,value FROM commit_record
    WHERE operation=?1 AND file_scope=?2 AND kind=?3 AND key>?4
    ORDER BY key LIMIT 64";
/// Every record of one operation up to one key, in one statement.
pub(crate) const INDEXED_OPERATION_RECORD_DELETE_OPERATION: &str = "DELETE FROM commit_record
    WHERE operation=?1 AND (file_scope,kind,key)<=(?2,?3,?4)";
/// Every record of one namespace up to one key, in one statement.
pub(crate) const INDEXED_OPERATION_RECORD_RECLAIM_OPERATION: &str =
    "SELECT file_scope,kind,key,length(value)
    FROM commit_record WHERE operation=?1 ORDER BY file_scope,kind,key LIMIT 64";

/// Whether layer ?2 of serial ?1 is installed below floor ?3 or superseded
/// by a later row up to the runtime active frontier ?4.
pub(crate) const LAYER_SUPERSEDED: &str = "SELECT ?2<=?3 OR EXISTS(SELECT 1
    FROM fs_inode i WHERE i.inode=?1 AND i.generation<=?4
    AND i.generation>max(?2,?3))";
