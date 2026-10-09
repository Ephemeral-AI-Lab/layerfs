//! Query plans of the native ownership points.
use crate::{db::integer, NativeMount, Overlay, OverlayResult, StatementKind};
impl Overlay {
    /// Actual indexed query plans for the native ownership points and bounded
    /// revoked lookup cursor; pair with diagnostics for work/complexity claims.
    pub fn explain_native(&self, mount: NativeMount) -> OverlayResult<Vec<String>> {
        self.state(mount.route)?;
        let mut plans = Vec::new();
        for (name, sql) in [
            ("lookup-point", "SELECT owner,nlookup,implicit FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial=?3"),
            ("retire-window", "SELECT serial,owner FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial>?3 ORDER BY serial LIMIT 64"),
        ] {
            plans.extend(self.query(StatementKind::Explain, &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&mount.route.ns, &integer(mount.owner)?, &0_i64], 24,
                |r| Ok(format!("{name}: {}", r.get::<_, String>(3)?)))?);
        }
        for (name, sql) in [
            ("retire-files", crate::maintenance::NATIVE_FILE_WINDOW),
            (
                "retire-directories",
                crate::maintenance::NATIVE_DIRECTORY_WINDOW,
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&mount.route.ns, &integer(mount.owner)?],
                16,
                |r| Ok(format!("{name}: {}", r.get::<_, String>(3)?)),
            )?);
        }
        Ok(plans)
    }
}
