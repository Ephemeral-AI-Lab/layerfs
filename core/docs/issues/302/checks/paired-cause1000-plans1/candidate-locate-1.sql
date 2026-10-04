SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN(?)
