//! The semantic-core boundary between the Yacas AST and product domains.
//!
//! This module deliberately starts small.  It does not introduce a second
//! expression tree and it does not move any domain algorithm yet.  A
//! `MathematicalObject` owns the current Yacas expression and carries a sparse
//! semantic overlay.  Domains will gradually return `Transition`s and
//! `RuleEvent`s through this boundary.

use std::collections::BTreeMap;
use std::rc::Rc;

use serde::Serialize;
use yacas_rs::env::Environment;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

use crate::engine::EngineError;
use crate::protocol::Condition;
use crate::protocol::ResultMetadata;
use crate::semantic::ValueKind;

/// Stable only for the lifetime of one computation.  It is intentionally not
/// a pointer or a symbol spelling so future AST rewrites can preserve object
/// identity without exposing engine storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// Monotonically increasing version of one mathematical object.  A path is
/// meaningful only together with this revision; a later rewrite must never
/// reinterpret an old event's focus against the new AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRevision(pub u64);

/// Cumulative normalization strength. A higher level includes the guarantees
/// of every preceding level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NormalizationLevel {
    Structural,
    Domain,
    Conditional,
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationMode {
    Safe,
    Operation(OperatorId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationMetadata {
    pub level: NormalizationLevel,
    pub assumptions: Vec<Condition>,
    pub mode: NormalizationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationState {
    pub revision: ObjectRevision,
    pub metadata: NormalizationMetadata,
}

/// A path into the current AST.  Paths are scoped to one computation and are
/// invalidated by a rewrite that changes their ancestor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ExpressionPath(Vec<usize>);

impl ExpressionPath {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn argument(&self, index: usize) -> Self {
        let mut path = self.0.clone();
        path.push(index);
        Self(path)
    }

    pub fn segments(&self) -> &[usize] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Atom,
    Number,
    Application,
    List,
    Generic,
}

/// Storage-independent, read-only AST view.  Creating a view only borrows the
/// existing engine node; it never allocates a mirror tree or reparses text.
pub struct ExpressionView<'a> {
    env: &'a Environment,
    node: &'a Rc<LispObject>,
}

impl<'a> ExpressionView<'a> {
    pub fn new(env: &'a Environment, node: &'a Rc<LispObject>) -> Self {
        Self { env, node }
    }

    pub fn kind(&self) -> NodeKind {
        match &self.node.kind {
            ObjectKind::Atom(_) => NodeKind::Atom,
            ObjectKind::Number(_) => NodeKind::Number,
            ObjectKind::Generic(_) => NodeKind::Generic,
            ObjectKind::Sublist(first) => {
                if first.atom_string().is_some() {
                    NodeKind::Application
                } else {
                    NodeKind::List
                }
            }
        }
    }

    pub fn head(&self) -> Option<&str> {
        match &self.node.kind {
            ObjectKind::Sublist(first) => first.atom_string().map(|head| head.as_ref()),
            _ => None,
        }
    }

    pub fn atom(&self) -> Option<&str> {
        self.node.atom_string().map(|atom| atom.as_ref())
    }

    pub fn number(&self) -> Option<String> {
        self.node.number_string()
    }

    pub fn arguments(&self) -> Vec<ExpressionView<'a>> {
        let ObjectKind::Sublist(first) = &self.node.kind else {
            return Vec::new();
        };
        spine_refs(first)
            .skip(1)
            .map(|node| ExpressionView {
                env: self.env,
                node,
            })
            .collect()
    }

    /// Compatibility boundary for an engine consumer that still needs source
    /// text.  Semantic-core internals should pass views, not this string.
    pub fn print_source(&self) -> String {
        yacas_rs::printer::infix_print(self.env, self.node)
    }
}

/// Stable identity of a product-level operation.  Spelling aliases and Yacas
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

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
    pub scope_argument: ValueArgument,
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

/// Semantic closure state for a bodied operator awaiting its value argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartialApplication {
    pub operator: OperatorId,
    pub spelling: String,
    pub expected_arity: usize,
    pub bound_arguments: Vec<BoundArgument>,
    pub missing: Vec<Requirement>,
    pub binder_scopes: Vec<BinderScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorDescriptor {
    pub id: OperatorId,
    pub names: &'static [&'static str],
    pub arities: &'static [usize],
    pub forms: &'static [ApplicationForm],
    pub value_argument: ValueArgument,
    pub binders: &'static [BinderDescriptor],
    pub capability: CapabilityId,
}

const CALL: &[ApplicationForm] = &[ApplicationForm::Call];
const BODIED: &[ApplicationForm] = &[ApplicationForm::Bodied];
const BODIED_AND_CONVENTIONAL: &[ApplicationForm] = &[
    ApplicationForm::Bodied,
    ApplicationForm::ConventionalValueFirst,
];
const NO_BINDERS: &[BinderDescriptor] = &[];
const FIRST_ARGUMENT_BINDS_LAST: &[BinderDescriptor] = &[BinderDescriptor {
    binder_argument: 0,
    scope_argument: ValueArgument::Last,
}];
const SECOND_ARGUMENT_BINDS_FIRST: &[BinderDescriptor] = &[BinderDescriptor {
    binder_argument: 1,
    scope_argument: ValueArgument::First,
}];

pub const OPERATOR_DESCRIPTORS: &[OperatorDescriptor] = &[
    OperatorDescriptor {
        id: OperatorId::Derivative,
        names: &["D", "Deriv"],
        arities: &[2, 3],
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::Differentiate,
    },
    OperatorDescriptor {
        id: OperatorId::Factor,
        names: &["Factor"],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::Factor,
    },
    OperatorDescriptor {
        id: OperatorId::AlgebraTransform,
        names: &["Expand", "Simplify", "Tidy"],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformAlgebra,
    },
    OperatorDescriptor {
        id: OperatorId::AlgebraTransform,
        names: &["Apart"],
        arities: &[2],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformAlgebra,
    },
    OperatorDescriptor {
        id: OperatorId::Integral,
        names: &["Integrate"],
        arities: &[2, 4],
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::Integrate,
    },
    OperatorDescriptor {
        id: OperatorId::Substitute,
        names: &["Subst"],
        arities: &[3],
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: NO_BINDERS,
        capability: CapabilityId::Substitute,
    },
    OperatorDescriptor {
        id: OperatorId::Limit,
        names: &["Limit"],
        arities: &[2, 3, 4],
        forms: BODIED_AND_CONVENTIONAL,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::EvaluateLimit,
    },
    OperatorDescriptor {
        id: OperatorId::Taylor,
        names: &["Taylor"],
        arities: &[3, 4],
        forms: BODIED_AND_CONVENTIONAL,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::ExpandTaylor,
    },
    OperatorDescriptor {
        id: OperatorId::Solve,
        names: &["Solve"],
        arities: &[2],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::SolveEquation,
    },
    OperatorDescriptor {
        id: OperatorId::MatrixTransform,
        names: &["Transpose", "Determinant", "Inverse"],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformMatrix,
    },
    OperatorDescriptor {
        id: OperatorId::MatrixSolve,
        names: &["MatrixSolve", "SolveMatrix"],
        arities: &[2],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformMatrix,
    },
    OperatorDescriptor {
        id: OperatorId::MatrixAnalyze,
        names: &[
            "Rank",
            "RREF",
            "RowReduce",
            "EigenValues",
            "NullSpace",
            "ColumnSpace",
            "EigenSpaces",
        ],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMatrix,
    },
    OperatorDescriptor {
        id: OperatorId::MatrixDecompose,
        names: &[
            "PLDU",
            "Cholesky",
            "GramSchmidt",
            "OrthogonalBasis",
            "OrthonormalBasis",
        ],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMatrix,
    },
    OperatorDescriptor {
        id: OperatorId::OdeSolve,
        names: &["OdeSolve"],
        arities: &[1],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::SolveOde,
    },
    OperatorDescriptor {
        id: OperatorId::Approximate,
        names: &["N", "Approximate"],
        arities: &[1, 2],
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::Approximate,
    },
];

pub fn operator_descriptor(name: &str) -> Option<&'static OperatorDescriptor> {
    OPERATOR_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.names.contains(&name))
}

/// Transitional recognition for product operations whose typed descriptor has
/// not migrated yet. Keeping it beside descriptors prevents execution layers
/// from growing independent string lists.
pub fn is_known_operator(name: &str) -> bool {
    operator_descriptor(name).is_some()
        || matches!(
            name,
            "DoubleIntegral"
                | "Extrema"
                | "FindRoot"
                | "ImproperIntegral"
                | "Lagrange"
                | "OdeSolve"
                | "OdeSolveNumeric"
                | "Plot"
                | "PolarIntegral"
                | "PrincipalValueIntegral"
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticInterpretation {
    PlainExpression,
    Operator {
        id: String,
    },
    Application {
        operator: String,
    },
    Equation,
    List,
    Matrix {
        rows: usize,
        columns: usize,
    },
    Vector {
        length: usize,
    },
    LinearSubspace {
        ambient_dimension: usize,
        basis_dimension: usize,
        kind: LinearSubspaceKind,
    },
    SpectralSubspaces {
        ambient_dimension: usize,
        space_count: usize,
    },
    MatrixFactorization {
        dimension: usize,
        kind: MatrixFactorizationKind,
        factor_count: usize,
        verified: bool,
    },
    OrderedBasis {
        ambient_dimension: usize,
        vector_count: usize,
        orthogonal: bool,
        normalized: bool,
        verified: bool,
    },
    SolutionSet {
        variables: Vec<String>,
        parameters: Vec<String>,
    },
    FunctionFamily {
        variable: String,
        parameters: Vec<String>,
    },
    /// A valid mathematical application deliberately retained because no
    /// closed-form evaluation is available yet (for example `Integrate`).
    HeldApplication {
        operator: String,
    },
    PartialApplication(PartialApplication),
    StructuredUnevaluated {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearSubspaceKind {
    NullSpace,
    ColumnSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixFactorizationKind {
    Pldu,
    Cholesky,
}

pub fn operand_partial_state(
    spelling: &str,
    bound_argument_count: usize,
) -> Result<PartialApplication, EngineError> {
    let descriptor = operator_descriptor(spelling)
        .ok_or_else(|| EngineError::InvalidInput(format!("未登记的偏应用运算符: {spelling}")))?;
    if descriptor.value_argument != ValueArgument::Last
        || !descriptor.forms.contains(&ApplicationForm::Bodied)
    {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 不是可等待 operand 的 bodied 运算符"
        )));
    }
    let expected_arity = bound_argument_count + 1;
    if !descriptor.arities.contains(&expected_arity) {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 不支持绑定 {bound_argument_count} 个参数后等待 operand"
        )));
    }
    Ok(PartialApplication {
        operator: descriptor.id,
        spelling: spelling.into(),
        expected_arity,
        bound_arguments: (0..bound_argument_count)
            .map(|slot| BoundArgument {
                slot,
                path: ExpressionPath::root().argument(slot),
            })
            .collect(),
        missing: vec![Requirement::Operand],
        binder_scopes: descriptor
            .binders
            .iter()
            .map(|binder| BinderScope {
                binder_slot: binder.binder_argument,
                scope_slot: expected_arity - 1,
            })
            .collect(),
    })
}

pub fn require_operand_partial<'a>(
    object: &'a MathematicalObject,
    expected: OperatorId,
) -> Result<&'a PartialApplication, EngineError> {
    let SemanticInterpretation::PartialApplication(partial) = &object.semantics.interpretation
    else {
        return Err(EngineError::InvalidInput(
            "对象不是等待 operand 的部分应用".into(),
        ));
    };
    if partial.operator != expected
        || partial.missing != [Requirement::Operand]
        || object.semantics.requirements != partial.missing
    {
        return Err(EngineError::InvalidInput(
            "部分应用的运算符或缺失槽位不匹配".into(),
        ));
    }
    crate::input::with_parse_env(|env| {
        let view = object.view(env);
        if view.head() != Some(partial.spelling.as_str())
            || view.arguments().len() != partial.bound_arguments.len()
        {
            return Err(EngineError::Parse(
                "部分应用语义状态与保留的 AST 不一致".into(),
            ));
        }
        Ok(())
    })?;
    Ok(partial)
}

/// Fill the missing operand slot by linking the operand AST into the retained
/// application. This constructs an application object; domain evaluation is a
/// separate transition.
pub fn complete_operand_partial(
    partial_object: &MathematicalObject,
    operand: &MathematicalObject,
) -> Result<MathematicalObject, EngineError> {
    let SemanticInterpretation::PartialApplication(partial) =
        &partial_object.semantics.interpretation
    else {
        return Err(EngineError::InvalidInput("对象不是部分应用".into()));
    };
    require_operand_partial(partial_object, partial.operator)?;
    let expression = partial_object.append_application_argument(operand)?;
    let mut completed = partial_object.clone();
    completed.apply(ObjectDelta {
        expression: Some(expression),
        semantics: Some(SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::Application {
                operator: partial.spelling.clone(),
            },
            metadata: ResultMetadata::unresolved(
                crate::semantic::Exactness::Symbolic,
                crate::protocol::OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }),
        overlay: None,
        normalization: None,
    });
    Ok(completed)
}

/// Operations a mathematical object may participate in.  This is deliberately
/// independent from its syntax and exactness: capabilities can be added as
/// domains migrate without creating a closed hierarchy of value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectCapability {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Negate,
    Differentiate,
    Integrate,
    Substitute,
    EvaluateLimit,
    Simplify,
    Factor,
    Expand,
    NumericEvaluate,
    Plot,
    ExpandTaylor,
    SolveEquation,
    SolveOde,
    MatrixAdd,
    MatrixMultiply,
    MatrixTranspose,
    MatrixDeterminant,
    MatrixInverse,
    MatrixSolve,
    MatrixAnalyze,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn symbolic_expression() -> Self {
        Self(
            (1 << ObjectCapability::Add as u8)
                | (1 << ObjectCapability::Subtract as u8)
                | (1 << ObjectCapability::Multiply as u8)
                | (1 << ObjectCapability::Divide as u8)
                | (1 << ObjectCapability::Power as u8)
                | (1 << ObjectCapability::Negate as u8)
                | (1 << ObjectCapability::Differentiate as u8)
                | (1 << ObjectCapability::Integrate as u8)
                | (1 << ObjectCapability::Substitute as u8)
                | (1 << ObjectCapability::EvaluateLimit as u8)
                | (1 << ObjectCapability::Simplify as u8)
                | (1 << ObjectCapability::Factor as u8)
                | (1 << ObjectCapability::Expand as u8)
                | (1 << ObjectCapability::NumericEvaluate as u8)
                | (1 << ObjectCapability::Plot as u8)
                | (1 << ObjectCapability::ExpandTaylor as u8)
                | (1 << ObjectCapability::SolveEquation as u8),
        )
    }
    pub const fn equation_input() -> Self {
        Self((1 << ObjectCapability::SolveEquation as u8) | (1 << ObjectCapability::SolveOde as u8))
    }
    pub const fn matrix() -> Self {
        Self(
            (1 << ObjectCapability::MatrixAdd as u8)
                | (1 << ObjectCapability::MatrixMultiply as u8)
                | (1 << ObjectCapability::MatrixTranspose as u8)
                | (1 << ObjectCapability::MatrixDeterminant as u8)
                | (1 << ObjectCapability::MatrixInverse as u8)
                | (1 << ObjectCapability::MatrixSolve as u8)
                | (1 << ObjectCapability::MatrixAnalyze as u8),
        )
    }
    pub const fn contains(self, capability: ObjectCapability) -> bool {
        self.0 & (1 << capability as u8) != 0
    }
}

/// Missing context is data, not an error or an invitation to stringify an
/// object. It lets partial applications remain composable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    Operand,
    Variable,
    ApproachPoint,
    Direction,
    Interval,
    InitialCondition,
    Precision,
    Assumption,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticState {
    pub kind: ValueKind,
    pub interpretation: SemanticInterpretation,
    pub metadata: ResultMetadata,
    pub capabilities: CapabilitySet,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticAnnotation {
    Operator { id: String },
    Equation { relation: String },
    Binding { name: String },
    GeneratedSymbol { id: String, display_name: String },
    DomainFact { name: String, value: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticOverlay {
    annotations: BTreeMap<ExpressionPath, SemanticAnnotation>,
}

impl SemanticOverlay {
    pub fn insert(&mut self, path: ExpressionPath, annotation: SemanticAnnotation) {
        self.annotations.insert(path, annotation);
    }

    pub fn get(&self, path: &ExpressionPath) -> Option<&SemanticAnnotation> {
        self.annotations.get(path)
    }

    pub fn annotations(&self) -> &BTreeMap<ExpressionPath, SemanticAnnotation> {
        &self.annotations
    }
}

#[derive(Clone)]
pub struct MathematicalObject {
    pub id: ObjectId,
    pub revision: ObjectRevision,
    expression: Rc<LispObject>,
    pub semantics: SemanticState,
    pub overlay: SemanticOverlay,
    pub normalization: Option<NormalizationState>,
}

impl MathematicalObject {
    pub fn new(id: ObjectId, expression: Rc<LispObject>, semantics: SemanticState) -> Self {
        Self {
            id,
            revision: ObjectRevision(0),
            expression,
            semantics,
            overlay: SemanticOverlay::default(),
            normalization: None,
        }
    }

    pub fn view<'a>(&'a self, env: &'a Environment) -> ExpressionView<'a> {
        ExpressionView::new(env, &self.expression)
    }

    /// Transitional serialization boundary for legacy domain engines. New
    /// semantic operations must receive this object, rather than a source
    /// string; only their explicitly marked engine adapter may print it.
    pub fn print_source(&self) -> String {
        crate::input::with_parse_env(|env| self.view(env).print_source())
    }

    pub(crate) fn raw_expression(&self) -> Rc<LispObject> {
        self.expression.clone()
    }

    pub(crate) fn rebuild_application_with(
        &self,
        arguments: &[MathematicalObject],
    ) -> Result<Rc<LispObject>, EngineError> {
        let ObjectKind::Sublist(first) = &self.expression.kind else {
            return Err(EngineError::Parse("对象不是 application AST".into()));
        };
        let head = spine_refs(first)
            .next()
            .ok_or_else(|| EngineError::Parse("application AST 缺少 head".into()))?;
        let mut kinds = Vec::with_capacity(arguments.len() + 1);
        kinds.push(yacas_rs::value::clone_kind(&head.kind));
        kinds.extend(
            arguments
                .iter()
                .map(|argument| yacas_rs::value::clone_kind(&argument.expression.kind)),
        );
        let chain = yacas_rs::value::build_list(kinds)
            .ok_or_else(|| EngineError::Parse("无法重建 application AST".into()))?;
        Ok(LispObject::new(ObjectKind::Sublist(chain)))
    }

    fn append_application_argument(
        &self,
        argument: &MathematicalObject,
    ) -> Result<Rc<LispObject>, EngineError> {
        let ObjectKind::Sublist(first) = &self.expression.kind else {
            return Err(EngineError::Parse("部分应用不是 application AST".into()));
        };
        let mut kinds = spine_refs(first)
            .map(|node| yacas_rs::value::clone_kind(&node.kind))
            .collect::<Vec<_>>();
        kinds.push(yacas_rs::value::clone_kind(&argument.expression.kind));
        let chain = yacas_rs::value::build_list(kinds)
            .ok_or_else(|| EngineError::Parse("无法补全 application AST".into()))?;
        Ok(LispObject::new(ObjectKind::Sublist(chain)))
    }

    pub fn reference(&self, focus: Option<ExpressionPath>) -> ObjectReference {
        ObjectReference {
            object: self.id,
            revision: self.revision,
            focus,
        }
    }

    pub fn meets_normalization(&self, minimum: NormalizationLevel) -> bool {
        self.normalization
            .as_ref()
            .is_some_and(|state| state.revision == self.revision && state.metadata.level >= minimum)
    }

    /// Apply syntax and semantic changes together.  Callers construct the
    /// delta completely before invoking this method, so no half-updated object
    /// can escape a transition.
    pub fn apply(&mut self, delta: ObjectDelta) {
        let changed = delta.expression.is_some()
            || delta.semantics.is_some()
            || delta.overlay.is_some()
            || delta.normalization.is_some();
        let normalization = delta.normalization;
        if let Some(expression) = delta.expression {
            self.expression = expression;
        }
        if let Some(semantics) = delta.semantics {
            self.semantics = semantics;
        }
        if let Some(overlay) = delta.overlay {
            self.overlay = overlay;
        }
        if changed {
            self.revision.0 += 1;
        }
        if changed {
            self.normalization = normalization.map(|metadata| NormalizationState {
                revision: self.revision,
                metadata,
            });
        }
    }
}

/// Transitional construction boundary for legacy string-based domains.  The
/// resulting object owns the parsed Yacas tree; migrated callers pass the
/// object onward instead of parsing its source again.
pub fn object_from_source(
    id: ObjectId,
    source: &str,
    semantics: SemanticState,
) -> Result<MathematicalObject, EngineError> {
    crate::input::validate_safe_text(source, "语义表达式")?;
    crate::input::with_parse_env(|env| {
        let expression = yacas_rs::parser::parse_expression(env, &format!("{source};"))
            .map_err(|error| EngineError::InvalidInput(format!("语义表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("语义表达式为空".into()))?;
        Ok(MathematicalObject::new(id, expression, semantics))
    })
}

#[derive(Clone, Default)]
pub struct ObjectDelta {
    pub(crate) expression: Option<Rc<LispObject>>,
    pub(crate) semantics: Option<SemanticState>,
    pub(crate) overlay: Option<SemanticOverlay>,
    pub(crate) normalization: Option<NormalizationMetadata>,
}

#[derive(Clone)]
pub struct Transition {
    pub input: ObjectReference,
    pub output: MathematicalObject,
    pub event: Option<RuleEvent>,
}

/// A versioned reference to an AST object at the instant a rule ran.  It is
/// intentionally light-weight: traces refer to computation-owned objects
/// rather than serializing another complete expression tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectReference {
    pub object: ObjectId,
    pub revision: ObjectRevision,
    pub focus: Option<ExpressionPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulePayload {
    Rewrite,
    Decompose,
    Decision,
    Inference,
    Verification,
    Convergence,
    Numeric,
    Structural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleImportance {
    Routine,
    Normal,
    Key,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvent {
    pub rule: String,
    pub input: ObjectReference,
    /// Other typed inputs for non-unary rules. `input` remains the primary
    /// subject for compatibility with unary domain traces.
    pub additional_inputs: Vec<ObjectReference>,
    pub output: ObjectReference,
    pub bindings: Vec<(String, String)>,
    pub conditions: Vec<Condition>,
    pub payload: RulePayload,
    pub importance: RuleImportance,
    /// Optional product hint.  It never determines mathematical state; it is
    /// only consumed when projecting a trace into a teaching step.
    pub presentation: Option<RulePresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulePresentation {
    pub expression: String,
    pub explanation: String,
    /// Product TeX may be intentionally more expressive than the engine's
    /// generic rendering of `expression` (for example a limit's subscript).
    /// It remains a projection hint and never carries mathematical state.
    pub tex_override: Option<String>,
}

pub trait EventSink {
    fn record(&mut self, event: RuleEvent);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    Off,
    Compact,
    Detailed,
}

#[derive(Debug, Default)]
pub struct VecEventSink {
    pub events: Vec<RuleEvent>,
}

impl EventSink for VecEventSink {
    fn record(&mut self, event: RuleEvent) {
        self.events.push(event);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Ui(String),
}

#[derive(Debug, Clone, Default)]
pub struct RuleTrace {
    pub events: Vec<RuleEvent>,
}

#[derive(Clone)]
pub enum ComputationOutput {
    /// A mathematical value that may be passed to a later semantic operation.
    Value(MathematicalObject),
    /// A valid mathematical application whose evaluation is deferred. It
    /// remains an operand for operations that declare compatible capability.
    Held(MathematicalObject),
    /// The computation has a mathematical conclusion but no value that may
    /// participate in a later operation (for example a two-sided DNE limit).
    NoValue(MathematicalObject),
    /// The computation intentionally produced no mathematical value.  Its
    /// effects are terminal and must not enter the composition data plane.
    EffectsOnly,
}

#[derive(Clone)]
pub struct Computation {
    pub output: ComputationOutput,
    pub trace: Option<RuleTrace>,
    pub certificates: Vec<Certificate>,
    pub effects: Vec<Effect>,
}

impl Computation {
    pub fn value(&self) -> Option<&MathematicalObject> {
        match &self.output {
            ComputationOutput::Value(object) => Some(object),
            ComputationOutput::Held(_)
            | ComputationOutput::NoValue(_)
            | ComputationOutput::EffectsOnly => None,
        }
    }

    pub fn subject(&self) -> Option<&MathematicalObject> {
        match &self.output {
            ComputationOutput::Value(object)
            | ComputationOutput::Held(object)
            | ComputationOutput::NoValue(object) => Some(object),
            ComputationOutput::EffectsOnly => None,
        }
    }
}

/// Common contract for every migrated mathematical domain.  It makes object
/// identity, revision and semantic state the data-plane boundary; text APIs
/// are allowed only as adapters around this contract.
pub trait SemanticOperation<Request> {
    fn minimum_input_normalization(&self) -> NormalizationLevel {
        NormalizationLevel::Structural
    }

    fn compute(
        &self,
        engine: &mut dyn crate::engine::Engine,
        input: &MathematicalObject,
        request: &Request,
    ) -> Result<Computation, crate::engine::EngineError>;
}

/// The minimal counterpart for structural operators such as `+` and `*`.
/// It is introduced only now because those operators provide concrete
/// evidence that unary `SemanticOperation` cannot represent provenance.
pub trait BinarySemanticOperation<Request> {
    fn minimum_input_normalization(&self) -> NormalizationLevel {
        NormalizationLevel::Structural
    }

    fn compute(
        &self,
        engine: &mut dyn crate::engine::Engine,
        left: &MathematicalObject,
        right: &MathematicalObject,
        request: &Request,
    ) -> Result<Computation, crate::engine::EngineError>;
}

pub trait UnarySemanticOperation<Request> {
    fn minimum_input_normalization(&self) -> NormalizationLevel {
        NormalizationLevel::Structural
    }

    fn compute(
        &self,
        engine: &mut dyn crate::engine::Engine,
        input: &MathematicalObject,
        request: &Request,
    ) -> Result<Computation, crate::engine::EngineError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use yacas_rs::parser::parse_expression;

    #[test]
    fn view_borrows_one_ast_without_reparsing_children() {
        let mut env = Environment::new();
        let tree = parse_expression(&mut env, "Taylor(Exp(x),0,2);")
            .unwrap()
            .unwrap();
        let view = ExpressionView::new(&env, &tree);
        assert_eq!(view.kind(), NodeKind::Application);
        assert_eq!(view.head(), Some("Taylor"));
        let args = view.arguments();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].head(), Some("Exp"));
        assert_eq!(args[0].arguments()[0].atom(), Some("x"));
        assert_eq!(args[1].number(), Some("0".into()));
    }

    #[test]
    fn transition_updates_ast_and_semantics_as_one_delta() {
        let mut env = Environment::new();
        let old = parse_expression(&mut env, "x;").unwrap().unwrap();
        let new = parse_expression(&mut env, "x+1;").unwrap().unwrap();
        let metadata = ResultMetadata::unresolved(
            crate::semantic::Exactness::Symbolic,
            crate::protocol::OutcomeReason::AlgorithmUncovered,
        );
        let mut object = MathematicalObject::new(
            ObjectId(1),
            old,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: metadata.clone(),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        object.apply(ObjectDelta {
            expression: Some(new),
            semantics: Some(SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::StructuredUnevaluated {
                    reason: "test".into(),
                },
                metadata,
                capabilities: CapabilitySet::empty(),
                requirements: Vec::new(),
            }),
            overlay: None,
            normalization: None,
        });
        assert_eq!(object.view(&env).print_source(), "x+1");
        assert_eq!(object.semantics.kind, ValueKind::Unevaluated);
        assert_eq!(object.revision, ObjectRevision(1));
    }

    #[test]
    fn rule_references_keep_the_ast_revision_that_was_observed() {
        let mut env = Environment::new();
        let initial = parse_expression(&mut env, "x;").unwrap().unwrap();
        let replacement = parse_expression(&mut env, "1;").unwrap().unwrap();
        let metadata = ResultMetadata::solved(
            crate::semantic::Exactness::Exact,
            crate::protocol::ConditionSet::empty(),
        );
        let mut object = MathematicalObject::new(
            ObjectId(7),
            initial,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: metadata.clone(),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        let before = object.reference(Some(ExpressionPath::root()));
        object.apply(ObjectDelta {
            expression: Some(replacement),
            semantics: Some(SemanticState {
                kind: ValueKind::Scalar,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            }),
            overlay: None,
            normalization: None,
        });
        let after = object.reference(Some(ExpressionPath::root()));
        assert_eq!(before.object, after.object);
        assert_eq!(before.revision, ObjectRevision(0));
        assert_eq!(after.revision, ObjectRevision(1));
    }

    #[test]
    fn partial_state_records_slots_requirements_and_binder_scope() {
        let partial = operand_partial_state("Limit", 3).unwrap();
        assert_eq!(partial.operator, OperatorId::Limit);
        assert_eq!(partial.expected_arity, 4);
        assert_eq!(partial.missing, vec![Requirement::Operand]);
        assert_eq!(partial.bound_arguments.len(), 3);
        assert_eq!(partial.bound_arguments[2].path.segments(), &[2]);
        assert_eq!(
            partial.binder_scopes,
            vec![BinderScope {
                binder_slot: 0,
                scope_slot: 3
            }]
        );
    }

    #[test]
    fn partial_state_is_driven_by_the_shared_operator_descriptor() {
        assert!(operand_partial_state("D", 1).is_ok());
        assert!(operand_partial_state("Integrate", 1).is_ok());
        assert!(operand_partial_state("Factor", 0).is_err());
        assert!(operand_partial_state("D", 3).is_err());
    }

    #[test]
    fn completing_a_partial_links_the_operand_ast_without_string_state() {
        let partial = object_from_source(
            ObjectId(11),
            "D(x)",
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::PartialApplication(
                    operand_partial_state("D", 1).unwrap(),
                ),
                metadata: ResultMetadata::unresolved(
                    crate::semantic::Exactness::Symbolic,
                    crate::protocol::OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: vec![Requirement::Operand],
            },
        )
        .unwrap();
        let operand = object_from_source(
            ObjectId(12),
            "x^2",
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::solved(
                    crate::semantic::Exactness::Symbolic,
                    crate::protocol::ConditionSet::empty(),
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let completed = complete_operand_partial(&partial, &operand).unwrap();
        assert_eq!(completed.print_source(), "D(x)x^2");
        assert!(completed.semantics.requirements.is_empty());
        assert!(matches!(completed.semantics.interpretation,
            SemanticInterpretation::Application { ref operator } if operator == "D"));
    }
}
