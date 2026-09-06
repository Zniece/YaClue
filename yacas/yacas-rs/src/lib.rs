//! Rust implementation of the yacas engine.
//!
//! Semantics are aligned with the upstream C++ engine (frozen snapshot under
//! `oracle/yacas`); the upstream sources serve as the reference for any
//! behavioral question. Contract docs live in PORTING-CONTRACT.md:
//! - Contract §1 value semantics: copies are exclusive; objects are owned
//!   values, not shared mutable cells.
//! - Contract §2 numeric layer: decimal mantissa+exponent representation
//!   (semantics-first; not a BigDecimal-style scale model).

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
