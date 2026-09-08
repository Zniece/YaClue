// 引擎适配接口：加工层和步骤层只依赖本模块。

mod model;
mod proxy;
mod repl;
mod rust;

pub use model::{Engine, EngineError, ErrorCode, ErrorResponse, EvalResult, Expr};
pub use proxy::RustEngineProxy;
pub use repl::{cpp_reference_available, ReplEngine};
pub use rust::RustEngine;

#[cfg(test)]
mod tests;
