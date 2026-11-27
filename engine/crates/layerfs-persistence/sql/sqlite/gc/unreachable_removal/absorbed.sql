DELETE FROM absorbed
 WHERE NOT EXISTS (SELECT 1 FROM commit_lineage cl WHERE cl.run = absorbed.run);
