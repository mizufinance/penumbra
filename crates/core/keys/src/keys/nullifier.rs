use decaf377::Fq;

pub const NK_LEN_BYTES: usize = 32;

/// Allows deriving the nullifier associated with a positioned piece of state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NullifierKey(pub Fq);
