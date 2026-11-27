UPDATE store_totals SET init_entries=$1,init_files=$2,init_bytes=$3,init_canonical=$4,init_stored=$5,init_metadata=$6,init_millis=$7 WHERE id=1 AND init_entries=0 RETURNING 1;
