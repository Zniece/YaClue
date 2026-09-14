//! Semantic execution boundary for product composition.
//!
//! This module deliberately knows nothing about teaching-step DTOs, localized messages,
//! transport DTOs or GUI effects. Product projection consumes its computation
//! in the parent `composition` module.

use crate::elaboration::ElaboratedInput;
use crate::engine::{Engine, EngineError};
use crate::semantic_core::{Computation, ComputationContext, TraceMode};

pub(super) fn execute_computation(
    engine: &mut dyn Engine,
    input: &ElaboratedInput,
    include_trace: bool,
) -> Result<Computation, EngineError> {
    let trace_mode = if include_trace {
        TraceMode::Detailed
    } else {
        TraceMode::Off
    };
    crate::execution_visitor::execute_elaborated_structure_with_context(
        engine,
        &input.root,
        ComputationContext::new(trace_mode),
    )
}
