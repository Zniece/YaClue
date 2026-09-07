//! Shared work limits for exact and precision-driven numeric operations.

/// Maximum decimal digits an operation may explicitly materialize.
pub(crate) const MAX_DECIMAL_WORK_DIGITS: u32 = 100_000;

/// Maximum binary precision accepted by precision conversion commands.
pub(crate) const MAX_BINARY_WORK_BITS: u64 = 332_193;

/// Maximum exact binary shift materialized by `MathMul2Exp`.
pub(crate) const MAX_EXACT_SHIFT_BITS: u64 = 1_000_000;
