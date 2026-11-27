SELECT height FROM absorbed WHERE run = $1 AND depth >= $2 ORDER BY depth LIMIT 1;
