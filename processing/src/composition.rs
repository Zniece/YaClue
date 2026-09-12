//! Bounded, inside-out dispatch for teaching chains made from a small set of
//! product operations. This is an execution protocol, not a second CAS AST.

use serde::Serialize;

use crate::algebra::{self, TransformKind};
use crate::engine::{Engine, EngineError};
use crate::input::{analyze_expression, strip_tex_delimiters, RootCall};
use crate::numeric;
use crate::ode::{self, OdeStatus};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    is_known_operator, object_from_source, operator_descriptor, CapabilitySet, ComputationOutput,
    MathematicalObject, ObjectCapability, ObjectId, OperatorDescriptor, SemanticInterpretation,
    SemanticOperation, SemanticState,
};
pub use crate::semantic_core::{OperatorId as CompositionOperator, ValueArgument};
use crate::steps::{
    derive_antiderivative_family_with_verbosity, derive_steps_order_with_verbosity, Step,
    StepImportance, StepVerbosity,
};

const MAX_COMPOSITION_DEPTH: usize = 16;
const DEFAULT_PRECISION: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionStatus {
    Completed,
    Unresolved,
    NoValue,
    Unsupported,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompositionResult {
    pub status: CompositionStatus,
    pub value: String,
    pub tex: String,
    pub steps: Vec<Step>,
    pub operators: Vec<CompositionOperator>,
    pub reason: Option<String>,
    pub arbitrary_constants: Vec<String>,
    pub held: Option<HeldApplication>,
    pub conditions: ConditionSet,
    pub outcome: Option<ResultMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeldApplication {
    pub source: String,
    pub operand_head: String,
    pub pending_operators: Vec<CompositionOperator>,
}

struct Operation {
    name: String,
    signature: &'static OperatorDescriptor,
    arguments: Vec<String>,
}

/// Cheap gate over the root analysis already performed by the product input
/// path. Ordinary single-operation requests do not enter the composition
/// parser or pay another traversal.
pub fn is_candidate(call: &RootCall) -> bool {
    let Some(signature) = operator_descriptor(&call.head) else {
        return false;
    };
    if !signature.arities.contains(&call.arguments.len()) {
        return true;
    }
    if is_conventional_value_form(signature, call.arguments.len()) {
        return true;
    }
    let value_index = value_index(signature, call.arguments.len());
    call.argument_heads
        .get(value_index)
        .and_then(|head| head.as_deref())
        .is_some_and(|head| is_known_operator(head))
}

/// Execute a supported nested chain. `None` means the expression contains
/// fewer than two registered operations, so callers can retain their existing
/// single-operation fast path.
pub fn execute_steps(
    engine: &mut dyn Engine,
    expression: &str,
    verbosity: StepVerbosity,
) -> Result<Option<CompositionResult>, EngineError> {
    let elaborated = crate::elaboration::elaborate_input(expression)?;
    execute_elaborated(engine, &elaborated, verbosity, true)
}

pub fn execute_elaborated(
    engine: &mut dyn Engine,
    input: &crate::elaboration::ElaboratedInput,
    verbosity: StepVerbosity,
    include_steps: bool,
) -> Result<Option<CompositionResult>, EngineError> {
    let has_native_descendant = crate::arithmetic::has_object_native_descendant(&input.root);
    let has_effect_descendant = crate::arithmetic::has_effect_descendant(&input.root);
    let root_is_native = matches!(&input.root.form,
        crate::elaboration::MathematicalForm::Application { head }
            if crate::semantic_core::is_object_native_operator(head));
    let structural_context = matches!(
        input.root.form,
        crate::elaboration::MathematicalForm::Structural { .. }
    ) || (matches!(
        input.root.form,
        crate::elaboration::MathematicalForm::Relation { .. }
            | crate::elaboration::MathematicalForm::Collection
    ) && (has_native_descendant || has_effect_descendant))
        || (matches!(&input.root.form,
            crate::elaboration::MathematicalForm::Application { head }
                if !is_known_operator(head))
            && (has_native_descendant || has_effect_descendant));
    if (structural_context || root_is_native)
        && crate::arithmetic::can_execute_elaborated_tree(&input.root)
    {
        let computation = crate::arithmetic::execute_elaborated_structure(engine, &input.root)?;
        let subject = computation
            .subject()
            .expect("structural computation always owns a mathematical object");
        let value = subject.print_source();
        let status = match computation.output {
            ComputationOutput::Held(_) => CompositionStatus::Unresolved,
            ComputationOutput::NoValue(_) => CompositionStatus::NoValue,
            ComputationOutput::Value(_) => CompositionStatus::Completed,
            ComputationOutput::EffectsOnly => {
                unreachable!("calculus and structures are mathematical")
            }
        };
        let tex = if matches!(
            status,
            CompositionStatus::Unresolved | CompositionStatus::NoValue
        ) {
            tex_code(&value)
        } else {
            strip_tex_delimiters(&engine.eval(&value)?.tex)
        };
        let steps = if include_steps {
            computation
                .trace
                .as_ref()
                .map(|trace| crate::steps::render_rule_trace(engine, trace, verbosity))
                .transpose()?
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let reason = match status {
            CompositionStatus::NoValue => Some("内层数学结论不存在，外层运算未执行。".into()),
            CompositionStatus::Unresolved
                if matches!(&input.root.form,
                    crate::elaboration::MathematicalForm::Application { head }
                        if operator_descriptor(head)
                            .is_some_and(|descriptor| descriptor.id == CompositionOperator::Derivative))
                    && !subject
                        .semantics
                        .capabilities
                        .contains(ObjectCapability::Differentiate) =>
            {
                Some("内层结果属于不可求导的扩展实数，外层求导保持未解析。".into())
            }
            CompositionStatus::Unresolved => Some("数学对象保持未解析，等待适用能力。".into()),
            _ => None,
        };
        let mut operators = Vec::new();
        collect_migrated_operator_ids(&input.root, &mut operators);
        let present_symbols = crate::input::with_parse_env(|env| {
            let semantic = crate::semantic::analyze_tree(env, &subject.raw_expression()).semantic;
            semantic
                .symbols
                .into_iter()
                .chain(semantic.constants)
                .collect::<Vec<_>>()
        });
        let arbitrary_constants = computation
            .trace
            .as_ref()
            .into_iter()
            .flat_map(|trace| &trace.events)
            .flat_map(|event| &event.bindings)
            .filter(|(name, value)| name == "constant" && present_symbols.contains(value))
            .map(|(_, value)| value.clone())
            .fold(Vec::new(), |mut constants, value| {
                if !constants.contains(&value) {
                    constants.push(value);
                }
                constants
            });
        return Ok(Some(CompositionResult {
            status,
            value,
            tex,
            steps,
            operators,
            reason,
            arbitrary_constants,
            held: None,
            conditions: subject.semantics.metadata.conditions.clone(),
            outcome: Some(subject.semantics.metadata.clone()),
        }));
    }
    let mut operations = Vec::new();
    let collected = collect_operations_elaborated(&input.root, &mut operations, 0)?;
    let mut occupied_symbols = input.analyzed.semantic.symbols.clone();
    occupied_symbols.extend(input.analyzed.semantic.bound_symbols.iter().cloned());
    occupied_symbols.extend(input.analyzed.semantic.constants.iter().cloned());
    occupied_symbols.sort();
    occupied_symbols.dedup();
    execute_collected(
        engine,
        operations,
        collected,
        input.root.object.print_source(),
        occupied_symbols,
        verbosity,
        include_steps,
    )
}

fn collect_migrated_operator_ids(
    expression: &crate::elaboration::ElaboratedObject,
    output: &mut Vec<CompositionOperator>,
) {
    for child in &expression.children {
        collect_migrated_operator_ids(child, output);
    }
    if let crate::elaboration::MathematicalForm::Application { head } = &expression.form {
        if crate::semantic_core::is_object_native_operator(head) {
            if let Some(descriptor) = operator_descriptor(head) {
                output.push(descriptor.id);
            }
        }
    }
}

fn execute_collected(
    engine: &mut dyn Engine,
    operations: Vec<Operation>,
    collected: (Result<String, String>, Option<String>),
    expression: String,
    mut occupied_symbols: Vec<String>,
    verbosity: StepVerbosity,
    include_steps: bool,
) -> Result<Option<CompositionResult>, EngineError> {
    let (leaf, leaf_head) = collected;
    let leaf = match leaf {
        Ok(leaf) => leaf,
        Err(reason) => {
            return Ok(Some(CompositionResult {
                status: CompositionStatus::Unsupported,
                value: expression.clone(),
                tex: String::new(),
                steps: Vec::new(),
                operators: operations
                    .iter()
                    .map(|operation: &Operation| operation.signature.id)
                    .collect(),
                reason: Some(reason),
                arbitrary_constants: Vec::new(),
                held: None,
                conditions: ConditionSet::empty(),
                outcome: None,
            }));
        }
    };
    let executable_single = operations.len() == 1
        && is_conventional_value_form(operations[0].signature, operations[0].arguments.len());
    if operations.len() < 2 && !executable_single {
        if let Some(operand_head) = leaf_head.filter(|head| is_known_operator(head)) {
            let pending_operators = operations
                .iter()
                .rev()
                .map(|operation| operation.signature.id)
                .collect();
            let step = operation_step(
                "held-operator-application",
                expression.clone(),
                "保留尚未降低的数学对象与外层运算，等待适用的组合规则。",
                tex_code(&expression),
            );
            return Ok(Some(CompositionResult {
                status: CompositionStatus::Unresolved,
                value: expression.clone(),
                tex: step.tex.clone(),
                steps: vec![step],
                operators: operations
                    .iter()
                    .map(|operation| operation.signature.id)
                    .collect(),
                reason: Some("组合在语义上有效，但当前没有适用的降低规则".into()),
                arbitrary_constants: Vec::new(),
                held: Some(HeldApplication {
                    source: expression.clone(),
                    operand_head,
                    pending_operators,
                }),
                conditions: ConditionSet::empty(),
                outcome: None,
            }));
        }
        return Ok(None);
    }

    // The first object-native composition slice.  This deliberately precedes
    // the legacy string protocol below: a solved Limit object is handed to
    // DerivativeOperation as an object with its identity/revision/capability,
    // never printed and reparsed by composition.
    if let Some(result) =
        execute_limit_then_derivative(engine, &operations, &leaf, verbosity, include_steps)?
    {
        return Ok(Some(result));
    }

    let mut current = leaf;
    let mut steps = Vec::new();
    let mut unresolved = false;
    let mut arbitrary_constants = Vec::new();
    for index in (0..operations.len()).rev() {
        let operation = &operations[index];
        occupied_symbols.extend(arbitrary_constants.iter().cloned());
        occupied_symbols.sort();
        occupied_symbols.dedup();
        let new_constant = (operation.signature.id == CompositionOperator::Integral
            && operation.arguments.len() == 2)
            .then(|| {
                crate::semantic::display_arbitrary_constants(&occupied_symbols, 1)
                    .into_iter()
                    .next()
                    .expect("one arbitrary constant was requested")
            });
        let outcome = apply(
            engine,
            operation,
            &current,
            new_constant.as_deref(),
            verbosity,
        )?;
        current = outcome.value;
        unresolved |= outcome.unresolved;
        arbitrary_constants.extend(outcome.arbitrary_constants);
        let active_symbols = analyze_expression(&current, "组合中间结果")?.symbols;
        arbitrary_constants.retain(|constant| active_symbols.contains(constant));
        steps.extend(wrap_pending_steps(outcome.steps, &operations[..index]));
    }
    let tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    arbitrary_constants.sort();
    arbitrary_constants.dedup();
    Ok(Some(CompositionResult {
        status: if unresolved {
            CompositionStatus::Unresolved
        } else {
            CompositionStatus::Completed
        },
        value: current,
        tex,
        steps,
        operators: operations
            .iter()
            .rev()
            .map(|operation| operation.signature.id)
            .collect(),
        reason: unresolved.then(|| "至少一个运算保持未求值".into()),
        arbitrary_constants,
        held: None,
        conditions: ConditionSet::empty(),
        outcome: None,
    }))
}

fn execute_limit_then_derivative(
    engine: &mut dyn Engine,
    operations: &[Operation],
    leaf: &str,
    verbosity: StepVerbosity,
    include_steps: bool,
) -> Result<Option<CompositionResult>, EngineError> {
    if operations.len() != 2
        || operations[0].signature.id != CompositionOperator::Derivative
        || operations[1].signature.id != CompositionOperator::Limit
    {
        return Ok(None);
    }
    let derivative = derivative_request(&operations[0])?;
    let limit = limit_request(&operations[1])?;
    let operand = composition_operand(leaf)?;
    let limited = crate::limits::LimitOperation.compute(engine, &operand, &limit)?;
    let limit_object = match &limited.output {
        ComputationOutput::Value(object) | ComputationOutput::Held(object) => object,
        ComputationOutput::NoValue(_object) => {
            let (steps, tex) = project_composition_trace(
                engine,
                limited.trace.as_ref().expect("Limit records a trace"),
                "Undefined",
                verbosity,
                include_steps,
            )?;
            return Ok(Some(CompositionResult {
                status: CompositionStatus::NoValue,
                // A NoValue must not masquerade as the held Limit AST or as
                // an operand for a later operation.  The trace still gives
                // the product its explanatory conclusion.
                value: "Undefined".into(),
                tex,
                steps,
                operators: vec![CompositionOperator::Limit, CompositionOperator::Derivative],
                reason: Some("内层极限不存在，不能作为求导操作数".into()),
                arbitrary_constants: Vec::new(),
                held: None,
                conditions: ConditionSet::empty(),
                outcome: None,
            }));
        }
        ComputationOutput::EffectsOnly => unreachable!("Limit always returns an object"),
    };
    if !limit_object
        .semantics
        .capabilities
        .contains(ObjectCapability::Differentiate)
    {
        let value = limit_object.print_source();
        let (steps, tex) = project_composition_trace(
            engine,
            limited.trace.as_ref().expect("Limit records a trace"),
            &value,
            verbosity,
            include_steps,
        )?;
        return Ok(Some(CompositionResult {
            status: CompositionStatus::Unresolved,
            value,
            tex,
            steps,
            operators: vec![CompositionOperator::Limit, CompositionOperator::Derivative],
            reason: Some("内层极限产生扩展实数，不能作为求导操作数".into()),
            arbitrary_constants: Vec::new(),
            held: None,
            conditions: limit_object.semantics.metadata.conditions.clone(),
            outcome: Some(limit_object.semantics.metadata.clone()),
        }));
    }
    let differentiated =
        crate::derivatives::DerivativeOperation.compute(engine, limit_object, &derivative)?;
    let value = differentiated
        .subject()
        .expect("derivative always returns an object")
        .print_source();
    let completed = matches!(&differentiated.output, ComputationOutput::Value(_));
    let result_conditions = differentiated
        .subject()
        .expect("derivative always returns an object")
        .semantics
        .metadata
        .conditions
        .clone();
    let result_outcome = differentiated
        .subject()
        .map(|object| object.semantics.metadata.clone());
    let mut events = limited.trace.expect("Limit records a trace").events;
    events.extend(
        differentiated
            .trace
            .expect("Derivative records a trace")
            .events,
    );
    let trace = crate::semantic_core::RuleTrace { events };
    let (steps, tex) = project_composition_trace(engine, &trace, &value, verbosity, include_steps)?;
    Ok(Some(CompositionResult {
        status: if completed {
            CompositionStatus::Completed
        } else {
            CompositionStatus::Unresolved
        },
        value,
        tex,
        steps,
        operators: vec![CompositionOperator::Limit, CompositionOperator::Derivative],
        reason: (!completed).then(|| "至少一个运算保持未求值".into()),
        arbitrary_constants: Vec::new(),
        held: None,
        conditions: result_conditions,
        outcome: result_outcome,
    }))
}

fn project_composition_trace(
    engine: &mut dyn Engine,
    trace: &crate::semantic_core::RuleTrace,
    value: &str,
    verbosity: StepVerbosity,
    include_steps: bool,
) -> Result<(Vec<Step>, String), EngineError> {
    if include_steps {
        let steps = crate::steps::render_rule_trace(engine, trace, verbosity)?;
        let tex = steps
            .last()
            .map(|step| step.tex.clone())
            .unwrap_or_default();
        Ok((steps, tex))
    } else {
        let tex = engine
            .render_tex_batch(&[value.to_string()])?
            .into_iter()
            .next()
            .map(|tex| strip_tex_delimiters(&tex))
            .unwrap_or_default();
        Ok((Vec::new(), tex))
    }
}

fn composition_operand(source: &str) -> Result<MathematicalObject, EngineError> {
    object_from_source(
        ObjectId(1),
        source,
        SemanticState {
            kind: ValueKind::Expression,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::unresolved(
                Exactness::Unknown,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        },
    )
}

fn derivative_request(
    operation: &Operation,
) -> Result<crate::derivatives::DerivativeRequest, EngineError> {
    let order = match operation.arguments.as_slice() {
        [_, _] => 1,
        [_, order, _] => order
            .parse()
            .map_err(|_| EngineError::InvalidInput("组合求导阶数必须是正整数".into()))?,
        _ => unreachable!("validated derivative arity"),
    };
    Ok(crate::derivatives::DerivativeRequest {
        variable: operation.arguments[0].clone(),
        order,
    })
}

fn limit_request(operation: &Operation) -> Result<crate::limits::LimitRequest, EngineError> {
    let (variable, at, direction) = match operation.arguments.as_slice() {
        [_, at] => ("x", at.as_str(), crate::limits::LimitDirection::Both),
        [variable, at, _] => (
            variable.as_str(),
            at.as_str(),
            crate::limits::LimitDirection::Both,
        ),
        [variable, at, direction, _] => (
            variable.as_str(),
            at.as_str(),
            match direction.as_str() {
                "Left" => crate::limits::LimitDirection::Left,
                "Right" => crate::limits::LimitDirection::Right,
                _ => {
                    return Err(EngineError::InvalidInput(
                        "组合极限方向应为 Left 或 Right".into(),
                    ))
                }
            },
        ),
        _ => unreachable!("validated limit arity"),
    };
    Ok(crate::limits::LimitRequest {
        variable: variable.into(),
        at: at.into(),
        direction,
    })
}

fn wrap_pending_steps(mut steps: Vec<Step>, pending: &[Operation]) -> Vec<Step> {
    for step in &mut steps {
        for operation in pending.iter().rev() {
            step.expr = wrap_expression(operation, &step.expr);
            step.tex = wrap_tex(operation, &step.tex);
        }
    }
    steps
}

fn wrap_expression(operation: &Operation, inner: &str) -> String {
    let value_index = value_index(operation.signature, operation.arguments.len());
    if is_conventional_value_form(operation.signature, operation.arguments.len()) {
        let mut arguments = operation.arguments.clone();
        arguments[0] = inner.into();
        return format!("{}({})", operation.name, arguments.join(","));
    }
    match operation.signature.value_argument {
        ValueArgument::First => {
            let mut arguments = operation.arguments.clone();
            arguments[value_index] = inner.into();
            format!("{}({})", operation.name, arguments.join(","))
        }
        ValueArgument::Last => format!(
            "{}({})({inner})",
            operation.name,
            operation.arguments[..value_index].join(",")
        ),
    }
}

fn wrap_tex(operation: &Operation, inner: &str) -> String {
    let value_index = value_index(operation.signature, operation.arguments.len());
    let fixed = operation
        .arguments
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != value_index)
        .map(|(_, argument)| tex_code(argument))
        .collect::<Vec<_>>()
        .join(",");
    let head = &operation.name;
    if fixed.is_empty() {
        format!(r"\operatorname{{{head}}}\!\left[{inner}\right]")
    } else {
        format!(r"\operatorname{{{head}}}_{{{fixed}}}\!\left[{inner}\right]")
    }
}

fn tex_code(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str(r"\backslash "),
            '{' => escaped.push_str(r"\{"),
            '}' => escaped.push_str(r"\}"),
            '_' => escaped.push_str(r"\_"),
            '^' => escaped.push_str(r"\^{}"),
            '%' | '#' | '&' | '$' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '~' => escaped.push_str(r"\sim "),
            _ => escaped.push(character),
        }
    }
    format!(r"\mathtt{{{escaped}}}")
}

/// AST-native counterpart of the legacy collector.  It deliberately consumes
/// elaborated children, so composition can be moved off its second parse one
/// route at a time.
fn collect_operations_elaborated(
    expression: &crate::elaboration::ElaboratedObject,
    operations: &mut Vec<Operation>,
    depth: usize,
) -> Result<(Result<String, String>, Option<String>), EngineError> {
    if depth >= MAX_COMPOSITION_DEPTH {
        return Ok((
            Err(format!("组合深度超过上限 {MAX_COMPOSITION_DEPTH}")),
            None,
        ));
    }
    let head = match &expression.form {
        crate::elaboration::MathematicalForm::Application { head }
        | crate::elaboration::MathematicalForm::EffectApplication { head } => head,
        crate::elaboration::MathematicalForm::Structural { operator } => {
            return Ok((Ok(expression.object.print_source()), Some(operator.clone())));
        }
        _ => return Ok((Ok(expression.object.print_source()), None)),
    };
    let Some(signature) = operator_descriptor(head) else {
        return Ok((Ok(expression.object.print_source()), Some(head.clone())));
    };
    if matches!(
        signature.availability,
        crate::semantic_core::OperatorAvailability::Pending(_)
    ) {
        return Ok((Ok(expression.object.print_source()), Some(head.clone())));
    }
    if !signature.arities.contains(&expression.children.len()) {
        return Ok((
            Err(format!(
                "{head} 不支持 {} 个参数",
                expression.children.len()
            )),
            None,
        ));
    }
    let value_index = value_index(signature, expression.children.len());
    let Some(inner) = expression.children.get(value_index) else {
        return Ok((Err(format!("{head} 缺少值参数")), None));
    };
    operations.push(Operation {
        name: head.clone(),
        signature,
        arguments: expression
            .children
            .iter()
            .map(|child| child.object.print_source())
            .collect(),
    });
    collect_operations_elaborated(inner, operations, depth + 1)
}

fn value_index(signature: &OperatorDescriptor, argument_count: usize) -> usize {
    if is_conventional_value_form(signature, argument_count) {
        return 0;
    }
    match signature.value_argument {
        ValueArgument::First => 0,
        ValueArgument::Last => argument_count.saturating_sub(1),
    }
}

fn is_conventional_value_form(signature: &OperatorDescriptor, argument_count: usize) -> bool {
    matches!(
        (signature.id, argument_count),
        (CompositionOperator::Limit, 2) | (CompositionOperator::Taylor, 3)
    )
}

struct ApplyOutcome {
    value: String,
    steps: Vec<Step>,
    unresolved: bool,
    arbitrary_constants: Vec<String>,
}

fn apply(
    engine: &mut dyn Engine,
    operation: &Operation,
    current: &str,
    new_constant: Option<&str>,
    verbosity: StepVerbosity,
) -> Result<ApplyOutcome, EngineError> {
    let arguments = &operation.arguments;
    match operation.signature.id {
        CompositionOperator::Derivative => {
            let order = if arguments.len() == 3 {
                arguments[1]
                    .parse::<u32>()
                    .map_err(|_| EngineError::InvalidInput("组合求导阶数必须是非负整数".into()))?
            } else {
                1
            };
            let mut steps = derive_steps_order_with_verbosity(
                engine,
                current,
                &arguments[0],
                order,
                verbosity,
            )?;
            if let Some(first) = steps.first_mut() {
                first.why = format!("对上一结果应用外层求导。{}", first.why);
            }
            from_steps(steps)
        }
        CompositionOperator::Integral => {
            if arguments.len() == 4 {
                let steps = crate::steps::derive_definite_with_verbosity(
                    engine,
                    current,
                    &arguments[0],
                    &arguments[1],
                    &arguments[2],
                    verbosity,
                )?;
                from_steps(steps)
            } else {
                let constant = new_constant
                    .ok_or_else(|| EngineError::Parse("组合不定积分缺少生成常数身份".into()))?;
                let result = derive_antiderivative_family_with_verbosity(
                    engine,
                    current,
                    &arguments[0],
                    constant.into(),
                    verbosity,
                )?;
                Ok(ApplyOutcome {
                    value: result.result.expression,
                    steps: result.steps,
                    unresolved: result.result.representative.starts_with("Integrate("),
                    arbitrary_constants: result.result.arbitrary_constants,
                })
            }
        }
        CompositionOperator::Factor => {
            let result = algebra::transform(engine, current, TransformKind::Factor, None)?;
            Ok(ApplyOutcome {
                value: result.output.clone(),
                steps: vec![operation_step(
                    "compose_factor",
                    result.output,
                    "对上一结果进行因式分解。",
                    result.tex,
                )],
                unresolved: result.unresolved,
                arbitrary_constants: Vec::new(),
            })
        }
        CompositionOperator::AlgebraTransform => {
            let (kind, variable) = match operation.name.as_str() {
                "Expand" => (TransformKind::Expand, None),
                "Simplify" => (TransformKind::Simplify, None),
                "Tidy" => (TransformKind::Tidy, None),
                "Apart" => (TransformKind::Apart, arguments.get(1).map(String::as_str)),
                _ => unreachable!("registered algebra transform"),
            };
            let result = algebra::transform(engine, current, kind, variable)?;
            Ok(ApplyOutcome {
                value: result.output.clone(),
                steps: vec![operation_step(
                    "compose_algebra_transform",
                    result.output,
                    "对上一结果应用代数变换。",
                    result.tex,
                )],
                unresolved: result.unresolved,
                arbitrary_constants: Vec::new(),
            })
        }
        CompositionOperator::Substitute => {
            let result = engine.eval(&format!(
                "Subst({},{})({current})",
                arguments[0], arguments[1]
            ))?;
            let value = result.expr.to_string();
            Ok(ApplyOutcome {
                steps: vec![operation_step(
                    "compose_substitute",
                    value.clone(),
                    "把指定值代入上一结果。",
                    strip_tex_delimiters(&result.tex),
                )],
                unresolved: value.starts_with("Subst("),
                value,
                arbitrary_constants: Vec::new(),
            })
        }
        CompositionOperator::Approximate => {
            let precision = arguments
                .get(1)
                .map(|value| {
                    value
                        .parse::<u32>()
                        .map_err(|_| EngineError::InvalidInput("组合近似精度必须是正整数".into()))
                })
                .transpose()?
                .unwrap_or(DEFAULT_PRECISION);
            let result = numeric::approximate(engine, current, precision)?;
            Ok(ApplyOutcome {
                value: result.output.clone(),
                steps: vec![operation_step(
                    "compose_approximate",
                    result.output,
                    "按指定精度计算上一结果的数值近似。",
                    result.tex,
                )],
                unresolved: matches!(result.kind, numeric::NumericKind::Unresolved),
                arbitrary_constants: Vec::new(),
            })
        }
        CompositionOperator::OdeSolve => {
            let result =
                ode::solve_steps_with_verbosity(engine, current, "x", "y", &[], verbosity)?;
            Ok(ApplyOutcome {
                value: result.result.solution.clone(),
                steps: result.steps,
                unresolved: result.result.status != OdeStatus::Solved,
                arbitrary_constants: result.result.constants,
            })
        }
        CompositionOperator::Limit => {
            let (variable, at) = if arguments.len() == 2 {
                ("x", arguments[1].as_str())
            } else {
                (arguments[0].as_str(), arguments[1].as_str())
            };
            let direction = if arguments.len() == 4 {
                match arguments[2].as_str() {
                    "Left" => crate::limits::LimitDirection::Left,
                    "Right" => crate::limits::LimitDirection::Right,
                    _ => {
                        return Err(EngineError::InvalidInput(
                            "组合极限方向应为 Left 或 Right".into(),
                        ));
                    }
                }
            } else {
                crate::limits::LimitDirection::Both
            };
            let steps = crate::limits::limit_steps_with_verbosity(
                engine, current, variable, at, direction, verbosity,
            )?;
            from_steps(steps)
        }
        CompositionOperator::Taylor => {
            let variable = if arguments.len() == 3 {
                "x"
            } else {
                arguments[0].as_str()
            };
            let degree = arguments[2]
                .parse::<u32>()
                .map_err(|_| EngineError::InvalidInput("组合 Taylor 次数必须是非负整数".into()))?;
            let result = numeric::taylor(engine, current, variable, &arguments[1], degree)?;
            Ok(ApplyOutcome {
                value: result.output.clone(),
                steps: vec![operation_step(
                    "compose_taylor",
                    result.output,
                    "对上一结果构造 Taylor 多项式。",
                    result.tex,
                )],
                unresolved: result.unresolved,
                arbitrary_constants: Vec::new(),
            })
        }
        CompositionOperator::Solve => Err(EngineError::InvalidInput(
            "Solve 必须通过类型化方程对象执行".into(),
        )),
        CompositionOperator::MatrixTransform => Err(EngineError::InvalidInput(
            "矩阵变换必须通过类型化矩阵对象执行".into(),
        )),
        CompositionOperator::MatrixSolve => Err(EngineError::InvalidInput(
            "线性方程组必须通过类型化矩阵对象执行".into(),
        )),
        CompositionOperator::MatrixAnalyze => Err(EngineError::InvalidInput(
            "矩阵结构分析必须通过类型化矩阵对象执行".into(),
        )),
        CompositionOperator::MatrixDecompose => Err(EngineError::InvalidInput(
            "矩阵分解必须通过类型化矩阵对象执行".into(),
        )),
        CompositionOperator::FactorProjection => Err(EngineError::InvalidInput(
            "分解因子必须通过类型化分解对象提取".into(),
        )),
        CompositionOperator::Sum => Err(EngineError::InvalidInput(
            "求和必须通过类型化数学对象执行".into(),
        )),
        CompositionOperator::ImproperIntegral
        | CompositionOperator::PrincipalValueIntegral
        | CompositionOperator::DoubleIntegral
        | CompositionOperator::PolarIntegral
        | CompositionOperator::OdeSolveNumeric
        | CompositionOperator::FindRoot
        | CompositionOperator::Plot
        | CompositionOperator::Extrema
        | CompositionOperator::Lagrange => Err(EngineError::InvalidInput(
            "待迁移运算符不能进入旧组合执行器".into(),
        )),
    }
}

fn from_steps(steps: Vec<Step>) -> Result<ApplyOutcome, EngineError> {
    let value = steps
        .last()
        .map(|step| step.expr.clone())
        .ok_or_else(|| EngineError::Parse("组合运算没有产生最终步骤".into()))?;
    let unresolved =
        value.starts_with("Integrate(") || value.starts_with("D(") || value.starts_with("Limit(");
    Ok(ApplyOutcome {
        value,
        steps,
        unresolved,
        arbitrary_constants: Vec::new(),
    })
}

fn operation_step(rule: &str, expr: String, why: &str, tex: String) -> Step {
    Step {
        rule: rule.into(),
        expr,
        why: why.into(),
        tex,
        importance: StepImportance::Key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn consumes_the_shared_operator_registry() {
        assert!(crate::semantic_core::OPERATOR_DESCRIPTORS
            .iter()
            .any(|item| item.names.contains(&"D")));
        assert!(crate::semantic_core::OPERATOR_DESCRIPTORS
            .iter()
            .any(|item| item.names.contains(&"Integrate")));
        assert!(crate::semantic_core::OPERATOR_DESCRIPTORS
            .iter()
            .any(|item| item.names.contains(&"Subst")));
        assert!(crate::semantic_core::OPERATOR_DESCRIPTORS
            .iter()
            .any(|item| item.names.contains(&"N")));
    }

    #[test]
    fn executes_root_structures_through_the_typed_object_pipeline() {
        let mut engine = RustEngine::spawn().unwrap();
        let input = crate::elaboration::elaborate_input("-(x-1)+2^3").unwrap();
        let result = execute_elaborated(&mut engine, &input, StepVerbosity::Standard, false)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert!(result.steps.is_empty());
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-(9-x))", result.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );

        let held = crate::elaboration::elaborate_input("(Limit(x,0)(f(x)))+1").unwrap();
        assert!(
            matches!(
                held.root.form,
                crate::elaboration::MathematicalForm::Structural { .. }
            ),
            "{:?}",
            held.root.form
        );
        let result = execute_elaborated(&mut engine, &held, StepVerbosity::Standard, false)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert!(result.value.contains("f(0)"));
    }

    #[test]
    fn sum_values_compose_inside_out_and_divergence_stops_outer_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        let input = crate::elaboration::elaborate_input("D(x)Sum(k,1,3,x*k)").unwrap();
        let result = execute_elaborated(&mut engine, &input, StepVerbosity::Detailed, true)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(result.value, "6");
        assert!(result.operators.contains(&CompositionOperator::Sum));
        assert!(result.operators.contains(&CompositionOperator::Derivative));
        assert!(result.steps.iter().any(|step| step.rule == "finite-sum"));

        let divergent = crate::elaboration::elaborate_input("D(x)Sum(k,1,Infinity,1/k)").unwrap();
        let result = execute_elaborated(&mut engine, &divergent, StepVerbosity::Detailed, false)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::NoValue);
    }

    #[test]
    fn executes_nested_calculus_inside_out() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)Integrate(x)x*Exp(x)",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(
            result.operators,
            [
                CompositionOperator::Integral,
                CompositionOperator::Derivative
            ]
        );
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-x*Exp(x))", result.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
        assert!(result.arbitrary_constants.is_empty());
        assert!(!result.value.contains(" + C"));
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "derivative-of-indefinite-integral"));
        assert!(result
            .steps
            .iter()
            .all(|step| step.rule != "antiderivative-family"));
    }

    #[test]
    fn generated_integral_constants_participate_in_later_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        let repeated = execute_steps(
            &mut engine,
            "Integrate(x)Integrate(x)x",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(repeated.status, CompositionStatus::Completed);
        assert_eq!(
            repeated.arbitrary_constants,
            ["C", "C1"],
            "{}",
            repeated.value
        );
        assert!(repeated.value.contains("C"));
        assert!(repeated.value.contains("C1"));
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify(ApplyPure(\"D\",{{x,ApplyPure(\"D\",{{x,{}}})}})-x)",
                    repeated.value
                ))
                .unwrap()
                .expr
                .to_string(),
            "0",
            "{}",
            repeated.value
        );

        let expanded = execute_steps(
            &mut engine,
            "Expand(Integrate(x)x)",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(expanded.arbitrary_constants, ["C"]);
        assert!(expanded.value.contains('C'));

        let collision = execute_steps(
            &mut engine,
            "Integrate(x)Integrate(x)C*x",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(collision.arbitrary_constants, ["C1", "C2"]);
        assert!(collision.value.contains("C"));
        assert!(collision.value.contains("C1"));
        assert!(collision.value.contains("C2"));
    }

    #[test]
    fn ode_results_cross_the_composition_boundary_after_normalization() {
        let mut engine = RustEngine::spawn().unwrap();
        for _ in 0..4 {
            let first_order =
                execute_steps(&mut engine, "D(x)OdeSolve(y'==y)", StepVerbosity::Standard)
                    .unwrap()
                    .unwrap();
            assert_eq!(first_order.status, CompositionStatus::Completed);
            assert_eq!(first_order.arbitrary_constants, ["C"]);
            assert!(first_order.value.contains('C'), "{first_order:#?}");
            assert!(!first_order.value.contains("C1"), "{first_order:#?}");
            assert!(
                !first_order.value.contains("UniqueSymbol"),
                "{first_order:#?}"
            );
        }

        let second_order = execute_steps(
            &mut engine,
            "D(x)OdeSolve(y''+4*y==Sin(x))",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(second_order.status, CompositionStatus::Completed);
        assert_eq!(second_order.arbitrary_constants, ["C1", "C2"]);
        assert!(
            !second_order.value.contains("Deriv(x,y"),
            "{second_order:#?}"
        );
        assert!(!second_order.value.contains("y(2)"), "{second_order:#?}");
    }

    #[test]
    fn ode_composition_preserves_user_constants_that_resemble_generated_names() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)OdeSolve(y'==y+C179*x)",
            StepVerbosity::Concise,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(result.arbitrary_constants, ["C"]);
        assert!(result.value.contains("C179"), "{result:#?}");
    }

    #[test]
    fn pending_outer_operations_remain_visible_during_inner_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)Factor(Integrate(x)2*x*(x^2+1))",
            StepVerbosity::Detailed,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert!(result.value.starts_with("D("), "{result:#?}");
        assert!(result.value.contains("Factor("), "{result:#?}");
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "hold-algebra-transform"));
        assert!(!result.value.contains("FWatom"), "{result:#?}");
    }

    #[test]
    fn covers_transform_substitution_and_approximation() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "N(Subst(x,2)(Integrate(x)Factor(D(x)x^2)),20)",
            StepVerbosity::Concise,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert!(result.value.contains('C'));
        assert_eq!(result.arbitrary_constants, ["C"]);
        assert_eq!(
            result.operators,
            [
                CompositionOperator::Derivative,
                CompositionOperator::Factor,
                CompositionOperator::Integral,
                CompositionOperator::Substitute,
                CompositionOperator::Approximate,
            ]
        );
    }

    #[test]
    fn lowers_registered_algebra_transforms_inside_compositions() {
        let mut engine = RustEngine::spawn().unwrap();
        for (expression, expected) in [
            ("D(x)Simplify((x+x)/2)", "1"),
            ("D(x)Expand((x+1)^2)", "((2*x)+2)"),
            ("D(x)Apart(1/(x^2-1),x)", "-2*x/(x^2-1)^2"),
            ("Integrate(x)Apart((x+1)/(x^2-1),x)", "Ln(x-1)+C"),
        ] {
            let result = execute_steps(&mut engine, expression, StepVerbosity::Concise)
                .unwrap()
                .unwrap();
            assert_eq!(result.status, CompositionStatus::Completed, "{expression}");
            assert_eq!(
                engine
                    .eval(&format!("Simplify(({})-({expected}))", result.value))
                    .unwrap()
                    .expr
                    .to_string(),
                "0",
                "{expression}: {result:#?}"
            );
            assert!(result.steps.iter().any(|step| matches!(
                step.rule.as_str(),
                "compose_algebra_transform"
                    | "apply-algebra-transform"
                    | "confirm-algebra-normal-form"
            )));
        }
    }

    #[test]
    fn lowers_limit_values_before_applying_outer_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)Limit(t,0)(Sin(t)/t+x^2)",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-2*x)", result.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
        let limit_result = result
            .steps
            .iter()
            .position(|step| step.rule == "limit-result")
            .unwrap();
        let derivative = result
            .steps
            .iter()
            .position(|step| step.rule == "sum-rule")
            .unwrap();
        assert!(limit_result < derivative);
    }

    #[test]
    fn refuses_to_differentiate_an_extended_real_limit_value() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)Limit(t,0,Right)(1/t+x^2)",
            StepVerbosity::Standard,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert_eq!(result.value, "D(x)Infinity");
        assert!(result
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("未解析")));
        assert!(result.steps.iter().all(|step| step.rule != "const-rule"));
    }

    #[test]
    fn preserves_a_nonexistent_limit_conclusion_without_applying_derivative() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(&mut engine, "D(x)Limit(t,0)(1/t)", StepVerbosity::Standard)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::NoValue);
        assert!(result
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("不存在")));
        assert!(result.steps.iter().any(|step| step.rule == "limit-result"));
        assert!(result.steps.iter().all(|step| step.rule != "const-rule"));
    }

    #[test]
    fn accepts_conventional_two_argument_limits_with_default_variable() {
        let mut engine = RustEngine::spawn().unwrap();
        for (expression, expected) in [("Limit(x,0)", "0"), ("Limit(Sin(x)/x,0)", "1")] {
            let result = execute_steps(&mut engine, expression, StepVerbosity::Standard)
                .unwrap()
                .unwrap();
            assert_eq!(result.status, CompositionStatus::Completed, "{expression}");
            assert_eq!(result.value, expected, "{expression}: {result:#?}");
            assert!(!result.steps.is_empty(), "{expression}");
            assert_eq!(result.steps.last().unwrap().expr, expected, "{expression}");
        }
    }

    #[test]
    fn accepts_conventional_three_argument_taylor_with_default_variable() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(&mut engine, "Taylor(Exp(x),0,6)", StepVerbosity::Standard)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify(({})-(1+x+x^2/2+x^3/6+x^4/24+x^5/120+x^6/720))",
                    result.value
                ))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
        assert!(result.steps.iter().any(|step| step.rule == "taylor-expand"));
    }

    #[test]
    fn lowers_taylor_values_before_applying_outer_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(
            &mut engine,
            "D(x)Taylor(x,0,4)Exp(x)",
            StepVerbosity::Concise,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-(1+x+x^2/2+x^3/6))", result.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
        assert!(result.steps.iter().any(|step| step.rule == "taylor-expand"));
    }

    #[test]
    fn malformed_registered_operation_has_structured_reason() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(&mut engine, "D(x,1,2,x)", StepVerbosity::Concise)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Unsupported);
        assert!(result.reason.unwrap().contains("4 个参数"));
    }

    #[test]
    fn single_migrated_operation_uses_the_object_native_path() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(&mut engine, "D(x)x^2", StepVerbosity::Concise)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(result.value, "2*x");
    }

    #[test]
    fn structured_operands_remain_held_when_no_lowering_rule_applies() {
        let mut engine = RustEngine::spawn().unwrap();
        for expression in [
            "Factor(DoubleIntegral(x+y,y,0,x,x,0,1))",
            "D(x)OdeSolveNumeric(y'==y,x,y,0,1,2)",
        ] {
            let result = execute_steps(&mut engine, expression, StepVerbosity::Concise)
                .unwrap()
                .unwrap();
            assert_eq!(result.status, CompositionStatus::Unresolved, "{expression}");
            assert_eq!(result.value, expression);
            assert_eq!(result.steps.len(), 1);
            assert_eq!(result.steps[0].rule, "held-operator-application");
            let held = result.held.as_ref().unwrap();
            assert_eq!(held.source, expression);
            assert!(!held.operand_head.is_empty());
            assert!(!held.pending_operators.is_empty());
            assert!(result
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("语义上有效")));
        }
    }

    #[test]
    fn solved_sets_are_typed_values_but_not_differentiable_operands() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = execute_steps(&mut engine, "D(x)Solve({x==1},{x})", StepVerbosity::Concise)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert!(result.value.starts_with("D(x)"), "{}", result.value);
        assert!(result.value.contains("x==1"), "{}", result.value);
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "solve-equations"));
    }
}
