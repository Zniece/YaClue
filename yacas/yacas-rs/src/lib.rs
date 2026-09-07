//! Rust implementation of the yacas engine.
//!
//! The Rust test suite defines supported behavior. Upstream Yacas can be
//! consulted for compatibility questions; intentional improvements are
//! documented and tested in this implementation.
//!
//! Value nodes are copied independently with shared immutable substructure.
//! The numeric layer uses a decimal mantissa/exponent representation.

pub mod assumptions;
pub mod commands;
pub mod containers;
pub mod env;
pub mod errors;
pub mod evaluator;
pub mod loader;
pub mod number;
pub mod operators;
pub mod parser;
pub mod pattern;
pub mod printer;
pub mod standard;
pub mod substitute;
pub mod symtab;
pub mod tokenizer;
pub mod userfunc;
pub mod value;
