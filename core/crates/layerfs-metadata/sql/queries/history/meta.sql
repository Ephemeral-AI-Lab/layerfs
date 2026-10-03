SELECT catalog_id,catalog_incarnation,identity_format,next_stage_token,binding_key,schema_version,schema_source,schema_definition FROM ${schema}.history_meta WHERE id = 1;
