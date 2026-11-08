//! Disclose Content's bounded LCS fallback without duplicating its diff algorithm.
use layerfs_content::compare::MAXIMUM_DIFF_CELLS;

pub fn exact(a: &[&str], b: &[&str]) -> bool {
    if (a.len() + 1).saturating_mul(b.len() + 1) <= MAXIMUM_DIFF_CELLS {
        return true;
    }
    let prefix = a.iter().zip(b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (a.len() - prefix - suffix + 1).saturating_mul(b.len() - prefix - suffix + 1)
        <= MAXIMUM_DIFF_CELLS
}
