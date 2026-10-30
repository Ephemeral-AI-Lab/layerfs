SELECT via,COUNT(*),SUM(new_canonical),SUM(new_stored),SUM(new_metadata) FROM "commit" GROUP BY via ORDER BY via;
