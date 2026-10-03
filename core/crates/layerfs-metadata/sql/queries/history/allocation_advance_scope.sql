UPDATE ${schema}.scope_allocator SET highwater = $1 WHERE scope_id = $2 AND highwater = $3 RETURNING 1;
