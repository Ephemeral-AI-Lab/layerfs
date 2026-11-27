SELECT path,frontier_serial,removed_ms FROM path_removed WHERE layer_stack_id=$1 ORDER BY path;
