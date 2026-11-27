UPDATE history_meta SET next_history_serial=next_history_serial+1 WHERE id=1 RETURNING next_history_serial-1;
