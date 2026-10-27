//! Only admission distinctions that change frontier waits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ServiceClass {
    General,
    Mutation,
    Source,
}
