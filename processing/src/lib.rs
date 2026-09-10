//! Processing layer: engine adapter traits, expressions, step generation,
//! and native plotting support.
//!
//! The engine boots the standard script library (`yacas/scripts`, loaded at
//! runtime); everything in this crate is original work licensed MIT.

pub mod algebra;
pub mod assumptions;
pub mod engine;
pub mod equations;
pub mod extrema;
pub mod input;
pub mod limits;
pub mod line_integrals;
pub mod linear_algebra;
pub mod multiple_integrals;
pub mod multivariate;
pub mod numeric;
pub mod ode;
pub mod ode_numeric;
pub mod plot;
pub mod quadrature;
pub mod semantic;
pub mod series;
pub mod steps;
pub mod surface_integrals;

#[cfg(test)]
mod test_support;
