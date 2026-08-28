//! Shared Store-open integrity policy.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IntegrityMode {
    #[default]
    Verified,
    TrustedLocalDev,
}
