use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::Serialize;
use yacas_rs::env::Environment;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

use crate::engine::EngineError;
use crate::protocol::ResultMetadata;
use crate::semantic::ValueKind;

use super::super::cache_session::{
    CachedOperationResult, OperationCache, OperationCacheBudget, OperationCacheEntry,
    OperationCacheKey, OperationSessionAst,
};
use super::super::normalization_representation::{
    NormalizationLevel, NormalizationMetadata, NormalizationState, RepresentationId,
    RepresentationPreference, RepresentationSet, StableRepresentation, MAX_STABLE_REPRESENTATIONS,
};
use super::super::operator_signature::{PartialApplication, TypedApplication};
use super::super::trace_computation::{ObjectReference, RuleEvent};
use super::{ExpressionPath, ExpressionView, MathematicalIdentity, ObjectId, ObjectRevision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticInterpretation {
    PlainExpression,
    Operator {
        id: String,
    },
    Application {
        operator: String,
    },
    TypedApplication(TypedApplication),
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
        dependent: Option<String>,
        parameters: Vec<String>,
    },
    NumericTrajectory(SampledTrajectory),
    /// A valid mathematical application deliberately retained because no
    /// closed-form evaluation is available yet (for example `Integrate`).
    HeldApplication {
        operator: String,
    },
    HeldTypedApplication(TypedApplication),
    PartialApplication(PartialApplication),
    StructuredUnevaluated {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SampledPoint {
    pub independent: String,
    pub state: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SampledTrajectory {
    pub independent: String,
    pub dependent: String,
    pub order: u32,
    pub points: Vec<SampledPoint>,
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
    SumSeries,
    IntegrateDefined,
    IntegrateMultiple,
    SolveNumericOde,
    FindNumericRoot,
    AnalyzeExtrema,
    AnalyzeMultivariate,
    IntegrateLine,
    IntegrateSurface,
    SolveEquation,
    SolveOde,
    MatrixAdd,
    MatrixMultiply,
    MatrixTranspose,
    MatrixDeterminant,
    MatrixInverse,
    MatrixSolve,
    MatrixAnalyze,
    ExtractFactors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilitySet(u64);

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
                | (1 << ObjectCapability::SumSeries as u8)
                | (1 << ObjectCapability::IntegrateDefined as u8)
                | (1 << ObjectCapability::IntegrateMultiple as u8)
                | (1 << ObjectCapability::SolveNumericOde as u8)
                | (1 << ObjectCapability::FindNumericRoot as u8)
                | (1 << ObjectCapability::AnalyzeExtrema as u8)
                | (1 << ObjectCapability::AnalyzeMultivariate as u8)
                | (1 << ObjectCapability::IntegrateLine as u8)
                | (1 << ObjectCapability::IntegrateSurface as u8)
                | (1 << ObjectCapability::SolveEquation as u8),
        )
    }
    pub const fn equation_input() -> Self {
        Self(
            (1 << ObjectCapability::SolveEquation as u8)
                | (1 << ObjectCapability::SolveOde as u8)
                | (1 << ObjectCapability::SolveNumericOde as u8),
        )
    }
    pub const fn collection() -> Self {
        Self(
            (1 << ObjectCapability::SolveEquation as u8)
                | (1 << ObjectCapability::SolveOde as u8)
                | (1 << ObjectCapability::SolveNumericOde as u8)
                | (1 << ObjectCapability::AnalyzeMultivariate as u8)
                | (1 << ObjectCapability::IntegrateLine as u8)
                | (1 << ObjectCapability::IntegrateSurface as u8),
        )
    }
    pub const fn matrix() -> Self {
        Self(
            (1 << ObjectCapability::MatrixAdd as u8)
                | (1 << ObjectCapability::MatrixMultiply as u8)
                | (1 << ObjectCapability::MatrixTranspose as u8)
                | (1 << ObjectCapability::MatrixDeterminant as u8)
                | (1 << ObjectCapability::MatrixInverse as u8)
                | (1 << ObjectCapability::MatrixSolve as u8)
                | (1 << ObjectCapability::MatrixAnalyze as u8)
                | (1 << ObjectCapability::AnalyzeMultivariate as u8)
                | (1 << ObjectCapability::IntegrateLine as u8)
                | (1 << ObjectCapability::IntegrateSurface as u8),
        )
    }
    pub const fn factorization() -> Self {
        Self(1 << ObjectCapability::ExtractFactors as u8)
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
    Order,
    LowerBound,
    UpperBound,
    Replacement,
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

pub struct MathematicalObject {
    pub id: ObjectId,
    pub revision: ObjectRevision,
    expression: Rc<LispObject>,
    pub semantics: SemanticState,
    pub overlay: SemanticOverlay,
    pub normalization: Option<NormalizationState>,
    representations: RepresentationSet,
    operation_cache: Rc<RefCell<Option<OperationCache>>>,
}

impl Clone for MathematicalObject {
    fn clone(&self) -> Self {
        crate::metrics::record_object_clone();
        Self {
            id: self.id,
            revision: self.revision,
            expression: self.expression.clone(),
            semantics: self.semantics.clone(),
            overlay: self.overlay.clone(),
            normalization: self.normalization.clone(),
            representations: self.representations.clone(),
            operation_cache: self.operation_cache.clone(),
        }
    }
}

impl MathematicalObject {
    pub fn new(id: ObjectId, expression: Rc<LispObject>, semantics: SemanticState) -> Self {
        let principal = StableRepresentation {
            id: RepresentationId(0),
            preference: RepresentationPreference::Principal,
            expression: expression.clone(),
            overlay: SemanticOverlay::default(),
            normalization: None,
        };
        Self {
            id,
            revision: ObjectRevision(0),
            expression,
            semantics,
            overlay: SemanticOverlay::default(),
            normalization: None,
            representations: RepresentationSet {
                candidates: vec![principal],
                active: RepresentationId(0),
                next_id: 1,
            },
            operation_cache: Rc::new(RefCell::new(None)),
        }
    }

    pub fn identity(&self) -> MathematicalIdentity {
        MathematicalIdentity(self.id)
    }

    pub fn active_representation(&self) -> RepresentationId {
        self.representations.active
    }

    pub fn stable_representation_count(&self) -> usize {
        self.representations.candidates.len()
    }

    pub fn representation_preference(
        &self,
        id: RepresentationId,
    ) -> Option<&RepresentationPreference> {
        self.representations
            .candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .map(|candidate| &candidate.preference)
    }

    /// Retain one explicitly produced equivalent representation. The caller
    /// supplies the transformation proof; B2 connects this primitive to
    /// certified algebra operations.
    #[allow(dead_code)] // B1 primitive; B2 is the first production caller.
    pub(crate) fn retain_representation(
        &mut self,
        expression: Rc<LispObject>,
        preference: RepresentationPreference,
        normalization: Option<NormalizationMetadata>,
    ) -> Option<RepresentationId> {
        let duplicate = crate::input::with_parse_env(|env| {
            let source = ExpressionView::new(env, &expression).print_source();
            self.representations
                .candidates
                .iter()
                .position(|candidate| {
                    ExpressionView::new(env, &candidate.expression).print_source() == source
                })
        });
        if let Some(index) = duplicate {
            let candidate = &mut self.representations.candidates[index];
            if candidate.id != RepresentationId(0) {
                candidate.preference = preference;
            }
            candidate.normalization = normalization;
            return Some(candidate.id);
        }
        if self.representations.candidates.len() >= MAX_STABLE_REPRESENTATIONS {
            let eviction = self
                .representations
                .candidates
                .iter()
                .position(|candidate| {
                    candidate.id != RepresentationId(0)
                        && candidate.id != self.representations.active
                })?;
            self.representations.candidates.remove(eviction);
            let id = RepresentationId(self.representations.next_id);
            self.representations.next_id += 1;
            self.representations.candidates.push(StableRepresentation {
                id,
                preference,
                expression,
                overlay: self.overlay.clone(),
                normalization,
            });
            return Some(id);
        }
        let id = RepresentationId(self.representations.next_id);
        self.representations.next_id += 1;
        self.representations.candidates.push(StableRepresentation {
            id,
            preference,
            expression,
            overlay: self.overlay.clone(),
            normalization,
        });
        Some(id)
    }

    #[allow(dead_code)] // B1 primitive; B2 is the first production caller.
    pub(crate) fn activate_representation(
        &mut self,
        id: RepresentationId,
    ) -> Result<(), EngineError> {
        let candidate = self
            .representations
            .candidates
            .iter()
            .find(|candidate| candidate.id == id)
            .cloned()
            .ok_or_else(|| EngineError::InvalidInput("未知的对象表示".into()))?;
        let already_active = self.representations.active == id
            && crate::input::with_parse_env(|env| {
                ExpressionView::new(env, &self.expression).print_source()
                    == ExpressionView::new(env, &candidate.expression).print_source()
            });
        if already_active {
            self.normalization = candidate.normalization.map(|metadata| NormalizationState {
                revision: self.revision,
                metadata,
            });
            return Ok(());
        }
        self.expression = candidate.expression;
        self.overlay = candidate.overlay;
        self.representations.active = id;
        crate::metrics::record_object_transition();
        self.revision.0 += 1;
        self.normalization = candidate.normalization.map(|metadata| NormalizationState {
            revision: self.revision,
            metadata,
        });
        Ok(())
    }

    pub fn operation_session(
        &self,
        representation: Option<RepresentationId>,
    ) -> Result<OperationSessionAst, EngineError> {
        crate::metrics::record_session_ast_handle();
        let expression = match representation {
            None => self.expression.clone(),
            Some(id) => self
                .representations
                .candidates
                .iter()
                .find(|candidate| candidate.id == id)
                .map(|candidate| candidate.expression.clone())
                .ok_or_else(|| EngineError::InvalidInput("未知的对象表示".into()))?,
        };
        Ok(OperationSessionAst {
            identity: self.identity(),
            revision: self.revision,
            expression,
        })
    }

    /// Start an ephemeral operation session from an explicitly derived AST.
    /// The AST is never retained as a stable representation by this method.
    pub(crate) fn temporary_operation_session(
        &self,
        expression: Rc<LispObject>,
    ) -> OperationSessionAst {
        crate::metrics::record_session_ast_handle();
        OperationSessionAst {
            identity: self.identity(),
            revision: self.revision,
            expression,
        }
    }

    pub fn operation_cache_key(
        &self,
        operation: impl Into<String>,
        assumptions: Vec<String>,
        precision: Option<u32>,
    ) -> OperationCacheKey {
        OperationCacheKey {
            operation: operation.into(),
            assumptions,
            precision,
            revision: self.revision,
            representation: self.active_representation(),
        }
    }

    pub(crate) fn cached_operation(
        &self,
        key: &OperationCacheKey,
    ) -> Option<CachedOperationResult> {
        self.operation_cache
            .borrow()
            .as_ref()?
            .entries
            .iter()
            .find(|entry| &entry.key == key)
            .map(|entry| entry.result.clone())
    }

    pub(crate) fn cache_operation(
        &self,
        key: OperationCacheKey,
        result: CachedOperationResult,
        budget: OperationCacheBudget,
    ) -> bool {
        let key_bytes = key
            .operation
            .len()
            .saturating_add(key.assumptions.iter().map(String::len).sum::<usize>());
        let bytes = key_bytes
            .saturating_add(result.source.len())
            .saturating_add(result.tex.len());
        if budget.max_entries == 0
            || bytes > budget.max_entry_bytes
            || bytes > budget.max_total_bytes
        {
            return false;
        }
        let mut storage = self.operation_cache.borrow_mut();
        let cache = storage.get_or_insert_with(OperationCache::default);
        if let Some(index) = cache.entries.iter().position(|entry| entry.key == key) {
            let replaced = cache.entries.remove(index);
            cache.total_bytes = cache.total_bytes.saturating_sub(replaced.bytes);
        }
        while !cache.entries.is_empty()
            && (cache.entries.len() >= budget.max_entries
                || cache.total_bytes.saturating_add(bytes) > budget.max_total_bytes)
        {
            let evicted = cache.entries.remove(0);
            cache.total_bytes = cache.total_bytes.saturating_sub(evicted.bytes);
        }
        cache.total_bytes = cache.total_bytes.saturating_add(bytes);
        cache
            .entries
            .push(OperationCacheEntry { key, result, bytes });
        true
    }

    pub fn cached_operation_count(&self) -> usize {
        self.operation_cache
            .borrow()
            .as_ref()
            .map_or(0, |cache| cache.entries.len())
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

    pub(crate) fn append_application_argument(
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
        let expression_changed = delta.expression.is_some();
        let changed = expression_changed
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
            crate::metrics::record_object_transition();
            self.revision.0 += 1;
        }
        if changed {
            self.normalization = normalization.map(|metadata| NormalizationState {
                revision: self.revision,
                metadata,
            });
        }
        if expression_changed {
            self.operation_cache = Rc::new(RefCell::new(None));
            self.representations = RepresentationSet {
                candidates: vec![StableRepresentation {
                    id: RepresentationId(0),
                    preference: RepresentationPreference::Principal,
                    expression: self.expression.clone(),
                    overlay: self.overlay.clone(),
                    normalization: self
                        .normalization
                        .as_ref()
                        .map(|state| state.metadata.clone()),
                }],
                active: RepresentationId(0),
                next_id: 1,
            };
        } else if changed {
            if let Some(active) = self
                .representations
                .candidates
                .iter_mut()
                .find(|candidate| candidate.id == self.representations.active)
            {
                active.overlay = self.overlay.clone();
                active.normalization = self
                    .normalization
                    .as_ref()
                    .map(|state| state.metadata.clone());
            }
        }
    }
}

/// Parse source returned by a string-based engine adapter back into the shared
/// AST representation without manufacturing a temporary semantic object.
pub(crate) struct ParsedEngineExpression {
    expression: Rc<LispObject>,
}

impl ParsedEngineExpression {
    pub(crate) fn raw_expression(&self) -> Rc<LispObject> {
        self.expression.clone()
    }
}

pub(crate) fn parse_engine_expression(source: &str) -> Result<ParsedEngineExpression, EngineError> {
    crate::input::validate_safe_text(source, "引擎表达式")?;
    crate::input::with_parse_env(|env| {
        let expression = yacas_rs::parser::parse_expression(env, &format!("{source};"))
            .map_err(|error| EngineError::InvalidInput(format!("引擎表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("引擎表达式为空".into()))?;
        Ok(ParsedEngineExpression { expression })
    })
}

/// Product/input construction boundary. Domain adapters that only need an AST
/// use `parse_engine_expression` and do not allocate a temporary object.
pub fn object_from_source(
    id: ObjectId,
    source: &str,
    semantics: SemanticState,
) -> Result<MathematicalObject, EngineError> {
    let parsed = parse_engine_expression(source)?;
    Ok(MathematicalObject::new(
        id,
        parsed.raw_expression(),
        semantics,
    ))
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
