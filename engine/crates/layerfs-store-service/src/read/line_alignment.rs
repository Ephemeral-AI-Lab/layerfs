//! Disclose Content's bounded LCS fallback without duplicating its diff algorithm.
use layerfs_content::compare::diff_exact;

pub fn exact(a: &[&str], b: &[&str]) -> bool {
    diff_exact(a, b)
}
