//! Type-safe bridges from operator descriptors to domain implementations.
//!
//! This module owns registration adapters only; mathematical algorithms stay
//! in their domain modules and traversal stays in `execution_visitor`.

use crate::arithmetic::{
    execute_calculus_application, execute_defined_integral_application, execute_effect_application,
    execute_extrema_application, execute_factor_projection_application,
    execute_find_root_application, execute_lagrange_application, execute_line_integral_application,
    execute_matrix_solve_application, execute_matrix_unary_application,
    execute_multiple_integral_application, execute_multivariate_application,
    execute_numeric_application, execute_numeric_ode_application, execute_ode_solve_application,
    execute_solve_application, execute_substitution_application, execute_sum_application,
    execute_surface_integral_application, execute_taylor_application,
    execute_transform_application, MultipleIntegralAdapterKind,
};
use crate::engine::{Engine, EngineError};
use crate::semantic_core::{Computation, RecursiveExecutionHandler};

pub(crate) fn execute_calculus_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_calculus_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_algebra_transform_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_transform_application(engine, recurse, expression, spelling)
}

macro_rules! execution_adapter {
    ($adapter:ident, $implementation:ident) => {
        pub(crate) fn $adapter(
            engine: &mut dyn Engine,
            expression: &crate::elaboration::ElaboratedObject,
            _spelling: &str,
            recurse: RecursiveExecutionHandler,
        ) -> Result<Computation, EngineError> {
            $implementation(engine, recurse, expression)
        }
    };
}

execution_adapter!(
    execute_substitution_adapter,
    execute_substitution_application
);
execution_adapter!(execute_taylor_adapter, execute_taylor_application);
execution_adapter!(execute_equation_solve_adapter, execute_solve_application);
execution_adapter!(execute_ode_solve_adapter, execute_ode_solve_application);
execution_adapter!(
    execute_factor_projection_adapter,
    execute_factor_projection_application
);
execution_adapter!(
    execute_matrix_solve_adapter,
    execute_matrix_solve_application
);
execution_adapter!(execute_series_adapter, execute_sum_application);
execution_adapter!(execute_numeric_ode_adapter, execute_numeric_ode_application);
execution_adapter!(execute_numeric_root_adapter, execute_find_root_application);

pub(crate) fn execute_approximate_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_numeric_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_matrix_unary_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_matrix_unary_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_improper_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_defined_integral_application(
        engine,
        recurse,
        expression,
        spelling,
        crate::improper_integrals::DefinedIntegralOperationKind::Improper,
    )
}

pub(crate) fn execute_principal_value_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_defined_integral_application(
        engine,
        recurse,
        expression,
        spelling,
        crate::improper_integrals::DefinedIntegralOperationKind::PrincipalValue,
    )
}

pub(crate) fn execute_double_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_multiple_integral_application(
        engine,
        recurse,
        expression,
        spelling,
        MultipleIntegralAdapterKind::Double,
    )
}

pub(crate) fn execute_polar_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_multiple_integral_application(
        engine,
        recurse,
        expression,
        spelling,
        MultipleIntegralAdapterKind::Polar,
    )
}

pub(crate) fn execute_plot_effect_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_effect_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_extrema_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    _spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_extrema_application(engine, recurse, expression)
}

pub(crate) fn execute_lagrange_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    _spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_lagrange_application(engine, recurse, expression)
}

pub(crate) fn execute_multivariate_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_multivariate_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_line_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_line_integral_application(engine, recurse, expression, spelling)
}

pub(crate) fn execute_surface_integral_adapter(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    spelling: &str,
    recurse: RecursiveExecutionHandler,
) -> Result<Computation, EngineError> {
    execute_surface_integral_application(engine, recurse, expression, spelling)
}
