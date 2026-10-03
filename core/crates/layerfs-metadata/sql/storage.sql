-- C2 PostgreSQL schema 1. Opaque payload descriptors, metadata bodies only.
-- No save identity/publication state, group table or dependency column.
BEGIN;
CREATE SCHEMA IF NOT EXISTS ${schema};
CREATE SEQUENCE ${schema}.pack_id AS BIGINT START 1 CACHE 1;
CREATE TABLE ${schema}.store_policy (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    schema_version SMALLINT NOT NULL CHECK (schema_version = 1),
    format_profile SMALLINT NOT NULL CHECK (format_profile = 1),
    small_file_threshold_bytes BIGINT NOT NULL CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
    whole_file_delta_max_depth SMALLINT NOT NULL CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
    chunk_delta_max_depth SMALLINT NOT NULL CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
    metadata_delta_max_depth SMALLINT NOT NULL CHECK (metadata_delta_max_depth BETWEEN 0 AND 50),
    next_ordinal BIGINT NOT NULL DEFAULT 1 CHECK (next_ordinal BETWEEN 1 AND 4294967296),
    metadata_window_start BIGINT NOT NULL DEFAULT 1 CHECK (metadata_window_start BETWEEN 1 AND 4294967295),
    metadata_window_values BIGINT NOT NULL DEFAULT 0 CHECK (metadata_window_values BETWEEN 0 AND 131072)
);
INSERT INTO ${schema}.store_policy(id,schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth)
VALUES (1,1,${format},${threshold},${whole_depth},${chunk_depth},${metadata_depth});
CREATE TABLE ${schema}.pack (
    pack_id BIGINT PRIMARY KEY CHECK (pack_id > 0),
    domain SMALLINT NOT NULL CHECK (domain IN (0,1)),
    digest BYTEA NOT NULL CHECK (octet_length(digest) = 32),
    length BIGINT NOT NULL CHECK (length BETWEEN 32 AND 16781312),
    body BYTEA,
    CHECK ((domain = 0 AND length <= 1048576 AND body IS NOT NULL AND octet_length(body) = length)
        OR (domain = 1 AND body IS NULL))
);
CREATE TABLE ${schema}.object (
    object_id BYTEA PRIMARY KEY CHECK (octet_length(object_id) = 32),
    role SMALLINT NOT NULL CHECK (role BETWEEN 1 AND 13),
    canonical_length BIGINT NOT NULL CHECK (canonical_length BETWEEN 1 AND 16777216),
    pack_id BIGINT NOT NULL REFERENCES ${schema}.pack(pack_id),
    group_number BIGINT NOT NULL CHECK (group_number BETWEEN 0 AND 255),
    record_number BIGINT NOT NULL CHECK (record_number BETWEEN 0 AND 8190)
);
CREATE TABLE ${schema}.metadata_value_group (
    first_ordinal BIGINT PRIMARY KEY CHECK (first_ordinal BETWEEN 1 AND 4294967295),
    count BIGINT NOT NULL CHECK (count BETWEEN 1 AND 165),
    pack_id BIGINT NOT NULL REFERENCES ${schema}.pack(pack_id),
    group_number BIGINT NOT NULL CHECK (group_number BETWEEN 0 AND 255),
    digest BYTEA NOT NULL CHECK (octet_length(digest) = 32),
    CHECK (first_ordinal + count <= 4294967296),
    UNIQUE (pack_id,group_number)
);
CREATE TABLE ${schema}.content_signature (
    slot BIGINT PRIMARY KEY CHECK (slot BETWEEN 0 AND 8191),
    stamp BIGINT NOT NULL CHECK (stamp > 0 AND (stamp - 1) % 8192 = slot),
    object_id BYTEA NOT NULL REFERENCES ${schema}.object(object_id),
    signature BYTEA NOT NULL CHECK (octet_length(signature) = 32)
);
CREATE FUNCTION ${schema}.reserve_storage(_packs BIGINT,_ordinals BIGINT)
RETURNS TABLE(first_pack_id BIGINT,first_ordinal BIGINT)
LANGUAGE plpgsql AS $body$
DECLARE cursor_ BIGINT;
BEGIN
    IF _packs < 0 OR _packs > 8191 OR _ordinals < 0 OR _ordinals > 131072 THEN
        RAISE EXCEPTION 'reservation bound' USING ERRCODE='22023';
    END IF;
    -- One row lock makes sequence block allocation and ordinal allocation atomic.
    SELECT next_ordinal INTO STRICT cursor_ FROM ${schema}.store_policy WHERE id=1 FOR UPDATE;
    IF cursor_ + _ordinals > 4294967296 THEN
        RAISE EXCEPTION 'ordinal maximum' USING ERRCODE='22003';
    END IF;
    first_ordinal := CASE WHEN _ordinals = 0 THEN 0 ELSE cursor_ END;
    first_pack_id := 0;
    IF _packs > 0 THEN
        first_pack_id := nextval('${schema}.pack_id'::regclass);
        PERFORM setval('${schema}.pack_id'::regclass,first_pack_id + _packs - 1,true);
    END IF;
    IF _ordinals > 0 THEN
        UPDATE ${schema}.store_policy SET next_ordinal=cursor_ + _ordinals WHERE id=1;
    END IF;
    RETURN NEXT;
END;
$body$;
CREATE FUNCTION ${schema}.register_storage(
    _p_ids BIGINT[],_p_domains SMALLINT[],_p_keys BYTEA[],_p_lengths BIGINT[],_p_bodies BYTEA[],
    _o_ids BYTEA[],_o_roles SMALLINT[],_o_lengths BIGINT[],_o_packs BIGINT[],_o_groups BIGINT[],_o_records BIGINT[],
    _g_first BIGINT[],_g_counts BIGINT[],_g_packs BIGINT[],_g_numbers BIGINT[],_g_digests BYTEA[],
    _s_slots BIGINT[],_s_stamps BIGINT[],_s_ids BYTEA[],_s_values BYTEA[],
    _window BIGINT,_release_first BIGINT,_release_count BIGINT)
RETURNS BYTEA[] LANGUAGE plpgsql AS $body$
DECLARE i INTEGER; lost_ BYTEA[] := ARRAY[]::BYTEA[];
BEGIN
    IF cardinality(_p_ids)+cardinality(_o_ids)+cardinality(_g_first)+cardinality(_s_slots)
        + (CASE WHEN _window IS NULL THEN 0 ELSE 1 END) + (CASE WHEN _release_first IS NULL THEN 0 ELSE 1 END) > 8191 THEN
        RAISE EXCEPTION 'registration rows' USING ERRCODE='22023';
    END IF;
    IF cardinality(_p_ids) <> cardinality(_p_domains) OR cardinality(_p_ids) <> cardinality(_p_keys)
        OR cardinality(_p_ids) <> cardinality(_p_lengths) OR cardinality(_p_ids) <> cardinality(_p_bodies)
        OR cardinality(_o_ids) <> cardinality(_o_roles) OR cardinality(_o_ids) <> cardinality(_o_lengths)
        OR cardinality(_o_ids) <> cardinality(_o_packs) OR cardinality(_o_ids) <> cardinality(_o_groups)
        OR cardinality(_o_ids) <> cardinality(_o_records) OR cardinality(_g_first) <> cardinality(_g_counts)
        OR cardinality(_g_first) <> cardinality(_g_packs) OR cardinality(_g_first) <> cardinality(_g_numbers)
        OR cardinality(_g_first) <> cardinality(_g_digests) OR cardinality(_s_slots) <> cardinality(_s_stamps)
        OR cardinality(_s_slots) <> cardinality(_s_ids) OR cardinality(_s_slots) <> cardinality(_s_values) THEN
        RAISE EXCEPTION 'registration arrays' USING ERRCODE='22023';
    END IF;
    IF COALESCE((SELECT sum(octet_length(body)) FROM unnest(_p_bodies) body),0)
        + COALESCE((SELECT sum(value) FROM unnest(_o_lengths) value),0) > 4194303
        AND NOT (cardinality(_o_ids)=1 AND cardinality(_p_ids)=1 AND _p_domains[1]=1) THEN
        RAISE EXCEPTION 'registration bytes' USING ERRCODE='22023';
    END IF;
    INSERT INTO ${schema}.pack(pack_id,domain,digest,length,body)
        SELECT * FROM unnest(_p_ids,_p_domains,_p_keys,_p_lengths,_p_bodies);
    -- The provider validates unique object IDs before issuing this bounded unit.
    -- RETURNING distinguishes a concurrent first-wins conflict without a second
    -- snapshot lookup. Keep lost identities in the caller's dependency order.
    WITH input AS MATERIALIZED (
        SELECT * FROM unnest(_o_ids,_o_roles,_o_lengths,_o_packs,_o_groups,_o_records)
            WITH ORDINALITY AS r(object_id,role,canonical_length,pack_id,group_number,record_number,position)
    ), inserted AS (
        INSERT INTO ${schema}.object(object_id,role,canonical_length,pack_id,group_number,record_number)
            SELECT object_id,role,canonical_length,pack_id,group_number,record_number FROM input ORDER BY position
            ON CONFLICT(object_id) DO NOTHING RETURNING object_id
    )
    SELECT COALESCE(array_agg(input.object_id ORDER BY position),ARRAY[]::BYTEA[]) INTO lost_
        FROM input WHERE NOT EXISTS (SELECT 1 FROM inserted WHERE inserted.object_id=input.object_id);
    INSERT INTO ${schema}.metadata_value_group(first_ordinal,count,pack_id,group_number,digest)
        SELECT * FROM unnest(_g_first,_g_counts,_g_packs,_g_numbers,_g_digests);
    -- Preserve the existing per-leaf/group value-count window recurrence.
    IF cardinality(_g_first)>0 OR _window IS NOT NULL OR _release_first IS NOT NULL THEN
        PERFORM id FROM ${schema}.store_policy WHERE id=1 FOR UPDATE;
        FOR i IN 1..cardinality(_g_first) LOOP
            UPDATE ${schema}.store_policy SET
                metadata_window_start=CASE WHEN metadata_window_values+_g_counts[i]>131072 THEN _g_first[i] ELSE metadata_window_start END,
                metadata_window_values=CASE WHEN metadata_window_values+_g_counts[i]>131072 THEN _g_counts[i] ELSE metadata_window_values+_g_counts[i] END
                WHERE id=1;
        END LOOP;
        IF _window IS NOT NULL THEN
            UPDATE ${schema}.store_policy SET metadata_window_start=GREATEST(metadata_window_start,_window) WHERE id=1;
        END IF;
        IF _release_first IS NOT NULL THEN
            IF _release_count IS NULL OR _release_count<0 THEN RAISE EXCEPTION 'ordinal release' USING ERRCODE='22023'; END IF;
            UPDATE ${schema}.store_policy SET next_ordinal=_release_first
                WHERE id=1 AND next_ordinal=_release_first+_release_count;
        END IF;
    END IF;
    INSERT INTO ${schema}.content_signature(slot,stamp,object_id,signature)
        SELECT * FROM unnest(_s_slots,_s_stamps,_s_ids,_s_values)
        ON CONFLICT(slot) DO UPDATE SET stamp=EXCLUDED.stamp,object_id=EXCLUDED.object_id,signature=EXCLUDED.signature
            WHERE EXCLUDED.stamp>=content_signature.stamp;
    RETURN lost_;
END;
$body$;
COMMIT;
