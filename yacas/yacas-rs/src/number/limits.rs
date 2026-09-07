//! Shared work limits for exact and precision-driven numeric operations.

/// Maximum decimal digits an operation may explicitly materialize.
pub(crate) const MAX_DECIMAL_WORK_DIGITS: u32 = 100_000;

/// Maximum binary precision accepted by precision conversion commands.
pub(crate) const MAX_BINARY_WORK_BITS: u64 = 332_193;

/// Absolute exponent ceiling for exact binary shifts. The decimal-work limit
/// may reject smaller exponents when the actual mantissa would exceed it.
pub(crate) const MAX_EXACT_SHIFT_BITS: u64 = 1_000_000;

/// Largest factorial whose decimal representation remains below the
/// decimal materialization limit (`25205!` has 99,996 digits).
pub(crate) const MAX_FACTORIAL_ARGUMENT: i64 = 25_205;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericWorkError {
    Interrupted,
    Overflow,
}
