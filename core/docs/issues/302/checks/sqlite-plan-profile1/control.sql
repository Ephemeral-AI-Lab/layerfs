SELECT CASE WHEN length(control) BETWEEN 24 AND 4120 THEN control END AS control,pack_id,domain,digest,length,length(control) FROM pack WHERE pack_id=1
