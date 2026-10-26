INSERT INTO scope_allocator (scope_id, highwater, authority_id) VALUES ($1, 0, $2) RETURNING 1;
