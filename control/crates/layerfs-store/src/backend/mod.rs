//! Where a Store lives: a sqlite file on a machine (`sqlite`) or a
//! PostgreSQL database (`postgresql`). The two never import each other.
pub mod postgresql;
pub mod sqlite;
