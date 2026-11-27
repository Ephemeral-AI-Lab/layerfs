SELECT phase,held_rows,held_bytes,peak_rows,peak_bytes,removed_rows,removed_bytes FROM {s}.init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint
