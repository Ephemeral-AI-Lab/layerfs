INSERT INTO binding_hint(child, parent, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING;
