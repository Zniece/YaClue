use serde::Serialize;

use crate::protocol::ConditionSet;

use super::object_state::{ExpressionPath, Requirement};
use super::trace_computation::Computation;

mod application;
mod registry;
pub use application::{
    complete_operand_partial, fill_next_partial_argument, operand_partial_state,
    partial_application_state, promote_held_application, promote_registered_held_expression,
    require_operand_partial, typed_application_state,
};
pub use registry::{
    is_known_operator, is_object_native_operator, object_native_route, operator_descriptor,
    OPERATOR_DESCRIPTORS,
};

/// Stable identity of a product-level operation. Spelling aliases and Yacas
/// surface forms belong to its descriptor rather than to dispatch callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorId {
    Derivative,
    Factor,
    AlgebraTransform,
    Integral,
    Substitute,
    Approximate,
    OdeSolve,
    Limit,
    Taylor,
    Solve,
    MatrixTransform,
    MatrixSolve,
    MatrixAnalyze,
    MatrixDecompose,
    FactorProjection,
    Sum,
    ImproperIntegral,
    PrincipalValueIntegral,
    DoubleIntegral,
    PolarIntegral,
    OdeSolveNumeric,
    FindRoot,
    Plot,
    Extrema,
    Lagrange,
    MultivariateDifferential,
    LineIntegral,
    SurfaceIntegral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    Differentiate,
    Factor,
    TransformAlgebra,
    Integrate,
    Substitute,
    Approximate,
    SolveOde,
    EvaluateLimit,
    ExpandTaylor,
    SolveEquation,
    TransformMatrix,
    AnalyzeMatrix,
    SumSeries,
    IntegrateDefined,
    IntegrateMultiple,
    SolveNumericOde,
    FindNumericRoot,
    RenderPlot,
    AnalyzeExtrema,
    AnalyzeMultivariate,
    IntegrateLine,
    IntegrateSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectNativeRoute {
    Calculus,
    AlgebraTransform,
    Substitute,
    Approximate,
    Taylor,
    EquationSolve,
    OdeSolve,
    MatrixUnary,
    MatrixSolve,
    FactorProjection,
    Series,
    DefinedIntegral,
    MultipleIntegral,
    NumericOde,
    NumericRoot,
    PlotEffect,
    Extrema,
    MultivariateDifferential,
    LineIntegral,
    SurfaceIntegral,
}

pub type RecursiveExecutionHandler = fn(
    &mut dyn crate::engine::Engine,
    &crate::elaboration::ElaboratedObject,
) -> Result<Computation, crate::engine::EngineError>;

pub type OperatorExecutionHandler = fn(
    &mut dyn crate::engine::Engine,
    &crate::elaboration::ElaboratedObject,
    &str,
    RecursiveExecutionHandler,
) -> Result<Computation, crate::engine::EngineError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationForm {
    Call,
    Bodied,
    ConventionalValueFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueArgument {
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinderDescriptor {
    pub binder_argument: usize,
    pub scope_argument: ScopeArgument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeArgument {
    First,
    Last,
    Index(usize),
}

/// A bound argument is addressed in the retained application AST, rather
/// than copied into semantic state as source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundArgument {
    pub slot: usize,
    pub path: ExpressionPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinderScope {
    pub binder_slot: usize,
    pub scope_slot: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ApplicationSlot {
    Bound {
        slot: usize,
        path: ExpressionPath,
    },
    Missing {
        slot: usize,
        requirement: Requirement,
    },
}

/// Semantic closure state for a bodied operator awaiting its value argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartialApplication {
    pub operator: OperatorId,
    pub spelling: String,
    pub expected_arity: usize,
    pub bound_arguments: Vec<BoundArgument>,
    /// Authoritative slot state. The older bound/missing projections remain
    /// serialized for protocol compatibility while consumers migrate.
    pub slots: Vec<ApplicationSlot>,
    pub missing: Vec<Requirement>,
    pub binder_scopes: Vec<BinderScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedArgument {
    pub slot: usize,
    pub path: ExpressionPath,
    pub requirement: Requirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTypeConstraint {
    Scalar,
    Expression,
    SameDomainAsOperand,
    DomainDefined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedApplication {
    pub operator: OperatorId,
    pub spelling: String,
    pub arguments: Vec<TypedArgument>,
    pub binder_scopes: Vec<BinderScope>,
    pub free_parameters: Vec<String>,
    pub conditions: ConditionSet,
    pub result: ResultTypeConstraint,
}

impl PartialApplication {
    pub fn display_template(&self, bound_sources: &[String]) -> String {
        let mut display = format!("{}({})", self.spelling, bound_sources.join(","));
        for requirement in &self.missing {
            let label = match requirement {
                Requirement::Operand => "operand",
                Requirement::Variable => "variable",
                Requirement::ApproachPoint => "approach_point",
                Requirement::Direction => "direction",
                Requirement::Interval => "interval",
                Requirement::Order => "order",
                Requirement::LowerBound => "lower_bound",
                Requirement::UpperBound => "upper_bound",
                Requirement::Replacement => "replacement",
                Requirement::InitialCondition => "initial_condition",
                Requirement::Precision => "precision",
                Requirement::Assumption => "assumption",
            };
            display.push_str(&format!("(<{label}>)"));
        }
        display
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OperatorDescriptor {
    pub id: OperatorId,
    pub names: &'static [&'static str],
    pub arities: &'static [usize],
    pub slot_signatures: &'static [OperatorSlotSignature],
    pub forms: &'static [ApplicationForm],
    pub value_argument: ValueArgument,
    pub binders: &'static [BinderDescriptor],
    pub capability: CapabilityId,
    pub route: ObjectNativeRoute,
    pub execution_handler: OperatorExecutionHandler,
    pub product_kind: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorSlotSignature {
    pub arity: usize,
    pub requirements: &'static [Requirement],
}

impl OperatorDescriptor {
    pub fn operand_index(&self, arity: usize) -> Option<usize> {
        self.slot_signatures
            .iter()
            .find(|signature| signature.arity == arity)?
            .requirements
            .iter()
            .position(|requirement| *requirement == Requirement::Operand)
    }

    pub fn product_presentation(
        &self,
        has_native_child: bool,
        completed: bool,
    ) -> Option<&'static str> {
        let visible = match self.route {
            ObjectNativeRoute::PlotEffect | ObjectNativeRoute::Extrema => true,
            ObjectNativeRoute::AlgebraTransform => !has_native_child && completed,
            ObjectNativeRoute::Substitute
            | ObjectNativeRoute::Approximate
            | ObjectNativeRoute::Taylor => false,
            _ => !has_native_child,
        };
        visible.then_some(self.product_kind)
    }
}
