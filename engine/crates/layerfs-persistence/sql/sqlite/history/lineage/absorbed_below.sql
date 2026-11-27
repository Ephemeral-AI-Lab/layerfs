SELECT depth FROM absorbed WHERE run = $1 AND height < $2 ORDER BY height DESC, depth DESC LIMIT 1;
