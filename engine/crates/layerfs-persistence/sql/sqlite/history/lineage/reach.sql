SELECT height FROM reach WHERE key = $1 AND x <= $2 ORDER BY x DESC LIMIT 1;
