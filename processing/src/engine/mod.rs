// 引擎适配接口：加工层和步骤层只依赖本模块。

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
