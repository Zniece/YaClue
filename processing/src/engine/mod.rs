// Engine boundary: processing and step projection depend only on this module.

mod config;
mod model;
mod proxy;
mod rust;

pub(crate) use model::canonical_call_ast;
pub use model::{Engine, EngineError, ErrorCode, ErrorResponse, EvalResult, Expr};
pub use proxy::RustEngineProxy;
pub use rust::RustEngine;

#[cfg(test)]
mod tests;
