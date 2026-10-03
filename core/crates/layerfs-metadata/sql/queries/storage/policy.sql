SELECT schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,
    chunk_delta_max_depth,metadata_delta_max_depth
FROM ${schema}.store_policy WHERE id=1;
