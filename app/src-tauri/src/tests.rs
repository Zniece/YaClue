use super::*;
use processing::binding::SymbolRole;
use std::collections::BTreeSet;

fn request(expression: &str, steps: bool) -> ProcessExpressionRequest {
    ProcessExpressionRequest {
        expression: expression.into(),
        steps,
        verbosity: "standard".into(),
    }
}

mod calculus_equations;
mod composition;
mod domain_products;
mod geometry;
mod gui_contract;
mod product_core;
