//! Numeric layer (contract §2): big naturals + floats (mantissa/exponent).
//!
//! Representation: a `Float` value = `digits × 10^(tens_exp − scale)` with a
//! decimal precision `prec`. Arithmetic carries one guard digit with
//! half-carry semantics; printing truncates (no carry-in) at the requested
//! digit count. `tens_exp == 0` prints verbatim (trailing zeros trimmed),
//! `tens_exp != 0` prints in e-form (mantissa in [0.1, 1)).

pub mod float;
pub mod nat;
