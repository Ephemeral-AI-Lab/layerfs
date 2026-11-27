SELECT h.catalog_incarnation,h.next_history_serial,p.next_pack_id,p.next_ordinal,
       t.logical,t.canonical,t.stored,t.metadata,
       (SELECT COALESCE(MAX(stamp),0) FROM content_signature),
       (SELECT COUNT(*) FROM "commit")+(SELECT COUNT(*) FROM layer)
  FROM history_meta h, store_policy p, store_totals t
 WHERE h.id=1 AND p.id=1 AND t.id=1
