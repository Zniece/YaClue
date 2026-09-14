use std::cell::Cell;

use serde::Serialize;

use crate::engine::EngineError;
use crate::protocol::Condition;

use super::normalization_representation::NormalizationLevel;
use super::object_state::{
    parse_engine_expression, ExpressionPath, MathematicalObject, ObjectDelta, ObjectId,
    ObjectRevision,
};

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

/// Product-independent classification of a trace event.
///
/// A trace records everything needed to explain and diagnose execution, while
/// a mathematical step is a much narrower product projection.  Keeping this
/// distinction in the semantic protocol prevents renderers from guessing
/// visibility from rule names or payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEventClass {
    /// A mathematical object is replaced by an equivalent representation.
    EquivalentTransformation,
    /// A diagnostic or mathematical observation used to choose a method.
    /// It may be useful to explain reasoning, but does not assert equality
    /// with either the preceding or following whole expression.
    MathematicalAnalysis,
    /// A computation reaches a mathematical conclusion that is not an
    /// equality-preserving rewrite (held, no value, unmet conditions, ...).
    MathematicalConclusion,
    /// Provenance required for execution/debugging but never a teaching step.
    InternalExecution,
    /// A product effect attached to the ordinary result path.
    ProductEffect,
}

impl RuleEventClass {
    /// Only genuine mathematical transformations participate in a continuous
    /// before -> after derivation. Conclusions and effects use separate
    /// product sections; internal execution stays trace-only.
    pub fn is_transformation_step(self) -> bool {
        matches!(self, Self::EquivalentTransformation)
    }

    pub fn is_product_visible(self) -> bool {
        !matches!(self, Self::InternalExecution)
    }
}

impl RuleEvent {
    /// Validate the semantic claim made by the event classification. This is
    /// intentionally independent of rule spelling and product rendering.
    pub fn validate_classification(&self) -> Result<(), &'static str> {
        match self.class {
            RuleEventClass::EquivalentTransformation
                if matches!(
                    self.payload,
                    RulePayload::Decision
                        | RulePayload::Inference
                        | RulePayload::Verification
                        | RulePayload::Convergence
                ) =>
            {
                Err("analysis payload cannot claim an equivalent transformation")
            }
            RuleEventClass::MathematicalAnalysis
                if matches!(self.payload, RulePayload::Rewrite | RulePayload::Decompose) =>
            {
                Err("rewrite payload must not be downgraded to analysis")
            }
            _ => Ok(()),
        }
    }
}

/// Materialize domain rule emissions as real, consecutive object revisions.
///
/// Domain algorithms may discover all emissions before constructing their
/// final semantic state. This boundary replays each emitted expression through
/// `ObjectDelta`, then attaches the final semantics to the last transition so
/// event references describe the versions that actually existed.
pub(crate) fn materialize_rule_transitions(
    input: &MathematicalObject,
    final_output: MathematicalObject,
    mut events: Vec<RuleEvent>,
    transition_expressions: &[String],
) -> Result<(MathematicalObject, Vec<RuleEvent>), EngineError> {
    if events.is_empty() {
        return Ok((final_output, events));
    }

    if transition_expressions.len() != events.len() {
        return Err(EngineError::Parse(format!(
            "规则事件与对象变换表达式数量不一致: events={}, expressions={}",
            events.len(),
            transition_expressions.len()
        )));
    }
    let final_expression = final_output.raw_expression();
    let final_normalization = final_output
        .normalization
        .as_ref()
        .map(|state| state.metadata.clone());
    let mut object = input.clone();
    let last = events.len() - 1;

    for (index, event) in events.iter_mut().enumerate() {
        let before = object.reference(None);
        let expression = if index == last {
            final_expression.clone()
        } else {
            parse_engine_expression(&transition_expressions[index])?.raw_expression()
        };
        object.apply(ObjectDelta {
            expression: Some(expression),
            semantics: (index == last).then(|| final_output.semantics.clone()),
            overlay: (index == last).then(|| final_output.overlay.clone()),
            normalization: if index == last {
                final_normalization.clone()
            } else {
                None
            },
        });
        let after = object.reference(None);
        event.input = before;
        event.output = after;
    }

    Ok((object, events))
}

/// Identity-based continuity contract for a chain of whole-expression
/// transformations. Text and TeX are projections and therefore cannot prove
/// continuity on their own.
pub fn transformation_chain_is_continuous(events: &[RuleEvent]) -> bool {
    events
        .windows(2)
        .all(|pair| pair[0].output == pair[1].input)
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
    pub class: RuleEventClass,
    pub input: ObjectReference,
    /// Other typed inputs for non-unary rules. `input` remains the primary
    /// subject for compatibility with unary domain traces.
    pub additional_inputs: Vec<ObjectReference>,
    pub output: ObjectReference,
    pub bindings: Vec<(String, String)>,
    pub conditions: Vec<Condition>,
    pub payload: RulePayload,
    pub importance: RuleImportance,
    /// Whole-expression replay data for product-visible transformations.
    /// Internal events deliberately leave this empty.
    pub transformation: Option<TransformationContext>,
    /// Optional product hint.  It never determines mathematical state; it is
    /// only consumed when projecting a trace into a teaching step.
    pub presentation: Option<RulePresentation>,
}

/// Authoritative mathematical fact emitted by a domain rule. Product-facing
/// text is deliberately absent: it is attached lazily by `EventSink` and can
/// never determine the computation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFact {
    pub rule: String,
    pub class: RuleEventClass,
    pub input: ObjectReference,
    pub additional_inputs: Vec<ObjectReference>,
    pub output: ObjectReference,
    pub bindings: Vec<(String, String)>,
    pub conditions: Vec<Condition>,
    pub payload: RulePayload,
    pub importance: RuleImportance,
    pub transformation: Option<TransformationContext>,
}

impl RuleFact {
    pub fn transition(
        rule: impl Into<String>,
        class: RuleEventClass,
        input: &MathematicalObject,
        output: &MathematicalObject,
        payload: RulePayload,
        importance: RuleImportance,
    ) -> Self {
        Self {
            rule: rule.into(),
            class,
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: Vec::new(),
            conditions: Vec::new(),
            payload,
            importance,
            transformation: None,
        }
    }

    pub fn with_bindings(mut self, bindings: Vec<(String, String)>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn with_conditions(mut self, conditions: Vec<Condition>) -> Self {
        self.conditions = conditions;
        self
    }

    fn into_event(self, presentation: Option<RulePresentation>) -> RuleEvent {
        RuleEvent {
            rule: self.rule,
            class: self.class,
            input: self.input,
            additional_inputs: self.additional_inputs,
            output: self.output,
            bindings: self.bindings,
            conditions: self.conditions,
            payload: self.payload,
            importance: self.importance,
            transformation: self.transformation,
            presentation,
        }
    }
}

impl RuleEvent {
    /// Returns explicit whole-expression context when an enclosing composer
    /// supplied it, otherwise a replayable root context for a unary/local
    /// rewrite. Multi-input rules must provide explicit context before they
    /// can be projected as an equivalence step.
    pub fn transformation_context(&self) -> Option<TransformationContext> {
        if let Some(context) = &self.transformation {
            return context.is_replayable(self).then(|| context.clone());
        }
        if self.presentation.is_none() || !self.additional_inputs.is_empty() {
            return None;
        }
        let focus = self
            .input
            .focus
            .clone()
            .unwrap_or_else(ExpressionPath::root);
        Some(TransformationContext {
            root_before: ObjectReference {
                focus: None,
                ..self.input.clone()
            },
            root_after: ObjectReference {
                focus: None,
                ..self.output.clone()
            },
            focus,
        })
    }
}

/// Versioned AST references needed to replay one local rewrite in the context
/// of the complete mathematical expression. The focus is interpreted against
/// `root_before`; `root_after` is the authoritative rewritten root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformationContext {
    pub root_before: ObjectReference,
    pub root_after: ObjectReference,
    pub focus: ExpressionPath,
}

impl TransformationContext {
    pub fn is_replayable(&self, event: &RuleEvent) -> bool {
        self.root_before.focus.is_none()
            && self.root_after.focus.is_none()
            && (!self.focus.segments().is_empty()
                || (self.root_before.revision == event.input.revision
                    && self.root_after.revision == event.output.revision))
    }
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

    fn record_fact(
        &mut self,
        fact: RuleFact,
        build_presentation: impl FnOnce() -> Option<RulePresentation>,
    ) where
        Self: Sized,
    {
        let presentation = if matches!(current_computation_context().trace_mode, TraceMode::Off) {
            None
        } else {
            build_presentation().inspect(|_| crate::metrics::record_rule_presentation())
        };
        self.record(fact.into_event(presentation));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    Off,
    Compact,
    Detailed,
}

/// Request-scoped execution policy shared by recursive composition and every
/// semantic operation. Mathematical behavior must not branch on `trace_mode`;
/// only trace materialization may do so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputationContext {
    pub trace_mode: TraceMode,
}

impl ComputationContext {
    pub const fn new(trace_mode: TraceMode) -> Self {
        Self { trace_mode }
    }
}

thread_local! {
    static ACTIVE_TRACE_MODE: Cell<TraceMode> = const { Cell::new(TraceMode::Detailed) };
}

pub fn current_computation_context() -> ComputationContext {
    ComputationContext::new(ACTIVE_TRACE_MODE.with(Cell::get))
}

pub fn with_computation_context<T>(
    context: ComputationContext,
    operation: impl FnOnce() -> T,
) -> T {
    ACTIVE_TRACE_MODE.with(|active| {
        let previous = active.replace(context.trace_mode);
        struct Restore<'a> {
            active: &'a Cell<TraceMode>,
            previous: TraceMode,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.active.set(self.previous);
            }
        }
        let _restore = Restore { active, previous };
        operation()
    })
}

pub fn materialize_presentation(
    build: impl FnOnce() -> RulePresentation,
) -> Option<RulePresentation> {
    (!matches!(current_computation_context().trace_mode, TraceMode::Off)).then(|| {
        crate::metrics::record_rule_presentation();
        build()
    })
}

pub fn materialize_presentation_if(
    enabled: bool,
    build: impl FnOnce() -> RulePresentation,
) -> Option<RulePresentation> {
    (enabled && !matches!(current_computation_context().trace_mode, TraceMode::Off)).then(|| {
        crate::metrics::record_rule_presentation();
        build()
    })
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

/// Versioned evidence emitted by a semantic operation. Known evidence is
/// represented by bounded variants; only out-of-tree extensions may carry an
/// open JSON document, and they must declare their protocol identity/version.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum CertificateEvidence {
    EquivalentRepresentation {
        operation: String,
        before: String,
        after: String,
    },
    NumericRootAttempt {
        status: crate::numeric::RootStatus,
        initial: f64,
        tolerance: f64,
        bracket: Option<(f64, f64)>,
    },
    MultivariateShape(Vec<usize>),
    LineIntegral(crate::line_integrals::LineIntegralResult),
    SurfaceIntegral(crate::surface_integrals::SurfaceIntegralResult),
    ExtremaAnalysis(crate::extrema::ExtremaResult),
    LagrangeAnalysis(crate::extrema::LagrangeResult),
    NumericOdeBudget {
        status: crate::ode_numeric::NumericOdeStatus,
        accepted_steps: usize,
        rejected_steps: usize,
        evaluations: usize,
        estimated_error: f64,
    },
    IntrinsicLowering(crate::intrinsics::LoweringCertificate),
    Extension(ExtensionCertificateEnvelope),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExtensionCertificateEnvelope {
    namespace: String,
    name: String,
    version: u32,
    document: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Certificate {
    version: u32,
    evidence: CertificateEvidence,
}

/// Product-facing analysis facts. The enum is strongly typed inside Rust but
/// remains untagged on the wire for compatibility with existing GUI and JSON
/// Lines consumers. Consumers may render these fields, but mathematical state
/// is determined by `SemanticState`/`ResultMetadata`, never by this projection.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ComputationAnalysis {
    MultivariateShape(Vec<usize>),
    LineIntegral(crate::line_integrals::LineIntegralResult),
    SurfaceIntegral(crate::surface_integrals::SurfaceIntegralResult),
    Extrema(crate::extrema::ExtremaResult),
    Lagrange(crate::extrema::LagrangeResult),
}

impl Certificate {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(evidence: CertificateEvidence) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            evidence,
        }
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn evidence(&self) -> &CertificateEvidence {
        &self.evidence
    }

    pub fn analysis(&self) -> Option<ComputationAnalysis> {
        match &self.evidence {
            CertificateEvidence::MultivariateShape(shape) => {
                Some(ComputationAnalysis::MultivariateShape(shape.clone()))
            }
            CertificateEvidence::LineIntegral(result) => {
                Some(ComputationAnalysis::LineIntegral(result.clone()))
            }
            CertificateEvidence::SurfaceIntegral(result) => {
                Some(ComputationAnalysis::SurfaceIntegral(result.clone()))
            }
            CertificateEvidence::ExtremaAnalysis(result) => {
                Some(ComputationAnalysis::Extrema(result.clone()))
            }
            CertificateEvidence::LagrangeAnalysis(result) => {
                Some(ComputationAnalysis::Lagrange(result.clone()))
            }
            _ => None,
        }
    }
}

impl ExtensionCertificateEnvelope {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: u32,
        document: serde_json::Value,
    ) -> Result<Self, &'static str> {
        let namespace = namespace.into();
        let name = name.into();
        if namespace.trim().is_empty() {
            return Err("extension certificate namespace cannot be empty");
        }
        if name.trim().is_empty() {
            return Err("extension certificate name cannot be empty");
        }
        if version == 0 {
            return Err("extension certificate version must be positive");
        }
        if !document.is_object() {
            return Err("extension certificate document must be an object");
        }
        Ok(Self {
            namespace,
            name,
            version,
            document,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Effect {
    Ui(String),
    Plot {
        effect: Box<crate::plot::PlotEffect>,
        semantic: crate::semantic::SemanticSummary,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RuleTrace {
    pub events: Vec<RuleEvent>,
}

impl RuleTrace {
    pub fn validate_classifications(&self) -> Result<(), &'static str> {
        self.events
            .iter()
            .try_for_each(RuleEvent::validate_classification)
    }

    pub fn apply_mode(&mut self, mode: TraceMode) {
        match mode {
            TraceMode::Detailed => {}
            TraceMode::Compact => {
                for event in &mut self.events {
                    if event.importance == RuleImportance::Routine {
                        event.presentation = None;
                    }
                }
            }
            TraceMode::Off => {
                for event in &mut self.events {
                    event.presentation = None;
                }
            }
        }
    }

    pub fn same_facts_as(&self, other: &Self) -> bool {
        self.events.len() == other.events.len()
            && self.events.iter().zip(&other.events).all(|(left, right)| {
                left.class == right.class
                    && left.rule == right.rule
                    && left.input == right.input
                    && left.additional_inputs == right.additional_inputs
                    && left.output == right.output
                    && left.bindings == right.bindings
                    && left.conditions == right.conditions
                    && left.payload == right.payload
                    && left.importance == right.importance
                    && left.transformation == right.transformation
            })
    }
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

    fn compute_with_context(
        &self,
        engine: &mut dyn crate::engine::Engine,
        input: &MathematicalObject,
        request: &Request,
        _context: &ComputationContext,
    ) -> Result<Computation, crate::engine::EngineError> {
        self.compute(engine, input, request)
    }
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

    fn compute_with_context(
        &self,
        engine: &mut dyn crate::engine::Engine,
        left: &MathematicalObject,
        right: &MathematicalObject,
        request: &Request,
        _context: &ComputationContext,
    ) -> Result<Computation, crate::engine::EngineError> {
        self.compute(engine, left, right, request)
    }
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

    fn compute_with_context(
        &self,
        engine: &mut dyn crate::engine::Engine,
        input: &MathematicalObject,
        request: &Request,
        _context: &ComputationContext,
    ) -> Result<Computation, crate::engine::EngineError> {
        self.compute(engine, input, request)
    }
}
