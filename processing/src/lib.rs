//! Processing layer: engine adapter traits, expressions, step generation,
//! and native plotting support.
//!
//! The engine boots the standard script library (`yacas/scripts`, loaded at
//! runtime); everything in this crate is original work licensed MIT.

pub mod algebra;
pub mod assumptions;
pub mod engine;
pub mod equations;
pub mod input;
pub mod limits;
pub mod numeric;
pub mod plot;
pub mod quadrature;
pub mod steps;
