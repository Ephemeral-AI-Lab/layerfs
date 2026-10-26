SELECT merged FROM commit_lineage WHERE run = $1 AND depth > $2 AND depth <= $3 AND merged IS NOT NULL;
