SELECT f.root FROM file_digest f JOIN object_location o ON o.object_id = f.root WHERE f.digest = $1 AND f.length = $2;
