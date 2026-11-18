SELECT pack_id,length FROM pack WHERE pack_id>$1 ORDER BY pack_id LIMIT $2
