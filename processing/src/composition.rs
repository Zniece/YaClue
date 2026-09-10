//! Bounded, inside-out dispatch for teaching chains made from a small set of
//! product operations. This is an execution protocol, not a second CAS AST.

use serde::Serialize;

use crate::algebra::{self, TransformKind};
use crate::engine::{Engine, EngineError};
use crate::input::{root_call, strip_tex_delimiters, validate_expression, RootCall};
use crate::numeric;
use crate::ode::{self, OdeStatus};
use crate::steps::{
    derive_integrals_with_verbosity, derive_steps_order_with_verbosity, Step, StepImportance,
    StepVerbosity,
};

const MAX_COMPOSITION_DEPTH: usize = 16;
const DEFAULT_PRECISION: u32 = 10;

/// Structured product operations are valid operands even before a lowering
/// rule exists for a particular outer operation. They must remain held rather
/// than falling through to raw Yacas evaluation.
const STRUCTURED_OPERATOR_NAMES: &[&str] = &[
    "Determinant",
    "DoubleIntegral",
    "EigenValues",
    "Extrema",
    "FindRoot",
    "ImproperIntegral",
    "Inverse",
    "Lagrange",
    "MatrixSolve",
    "OdeSolve",
    "OdeSolveNumeric",
    "Plot",
    "PolarIntegral",
    "PrincipalValueIntegral",
    "Solve",
    "SolveMatrix",
    "Transpose",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionOperator {
    Derivative,
    Factor,
    AlgebraTransform,
    Integral,
    Substitute,
    Approximate,
    OdeSolve,
    Limit,
    Taylor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OperatorSignature {
    pub name: &'static str,
    pub operator: CompositionOperator,
    pub arities: &'static [usize],
    /// Argument occupied by the value produced by the inner operation.
    pub value_argument: ValueArgument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueArgument {
    First,
    Last,
}

const D_ARITIES: &[usize] = &[2, 3];
const UNARY_ARITY: &[usize] = &[1];
const INTEGRATE_ARITIES: &[usize] = &[2, 4];
const SUBST_ARITIES: &[usize] = &[3];
const APPROXIMATE_ARITIES: &[usize] = &[1, 2];
const LIMIT_ARITIES: &[usize] = &[3, 4];
const TAYLOR_ARITIES: &[usize] = &[4];

pub const OPERATOR_SIGNATURES: &[OperatorSignature] = &[
    OperatorSignature {
        name: "D",
        operator: CompositionOperator::Derivative,
        arities: D_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "Deriv",
        operator: CompositionOperator::Derivative,
        arities: D_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "Factor",
        operator: CompositionOperator::Factor,
        arities: UNARY_ARITY,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Expand",
        operator: CompositionOperator::AlgebraTransform,
        arities: UNARY_ARITY,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Simplify",
        operator: CompositionOperator::AlgebraTransform,
        arities: UNARY_ARITY,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Tidy",
        operator: CompositionOperator::AlgebraTransform,
        arities: UNARY_ARITY,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Apart",
        operator: CompositionOperator::AlgebraTransform,
        arities: &[2],
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Integrate",
        operator: CompositionOperator::Integral,
        arities: INTEGRATE_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "Subst",
        operator: CompositionOperator::Substitute,
        arities: SUBST_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "Limit",
        operator: CompositionOperator::Limit,
        arities: LIMIT_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "Taylor",
        operator: CompositionOperator::Taylor,
        arities: TAYLOR_ARITIES,
        value_argument: ValueArgument::Last,
    },
    OperatorSignature {
        name: "OdeSolve",
        operator: CompositionOperator::OdeSolve,
        arities: UNARY_ARITY,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "N",
        operator: CompositionOperator::Approximate,
        arities: APPROXIMATE_ARITIES,
        value_argument: ValueArgument::First,
    },
    OperatorSignature {
        name: "Approximate",
        operator: CompositionOperator::Approximate,
        arities: APPROXIMATE_ARITIES,
        value_argument: ValueArgument::First,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionStatus {
    Completed,
    Unresolved,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct HeldApplication {
    pub source: String,
    pub operand_head: String,
    pub pending_operators: Vec<CompositionOperator>,
}

struct Operation {
    signature: &'static OperatorSignature,
    arguments: Vec<String>,
}

/// Cheap gate over the root analysis already performed by the product input
/// path. Ordinary single-operation requests do not enter the composition
/// parser or pay another traversal.
pub fn is_candidate(call: &RootCall) -> bool {
    let Some(signature) = OPERATOR_SIGNATURES
        .iter()
        .find(|signature| signature.name == call.head)
    else {
        return false;
    };
    if !signature.arities.contains(&call.arguments.len()) {
        return true;
    }
    let value_index = value_index(signature, call.arguments.len());
    call.argument_heads
        .get(value_index)
        .and_then(|head| head.as_deref())
        .is_some_and(|head| {
            OPERATOR_SIGNATURES.iter().any(|item| item.name == head)
                || STRUCTURED_OPERATOR_NAMES.contains(&head)
        })
}

/// Execute a supported nested chain. `None` means the expression contains
/// fewer than two registered operations, so callers can retain their existing
/// single-operation fast path.
pub fn execute_steps(
    engine: &mut dyn Engine,
    expression: &str,
    verbosity: StepVerbosity,
) -> Result<Option<CompositionResult>, EngineError> {
    validate_expression(expression, "组合表达式")?;
    let mut operations = Vec::new();
    let leaf = match collect_operations(expression, &mut operations, 0)? {
        Ok(leaf) => leaf,
        Err(reason) => {
            return Ok(Some(CompositionResult {
                status: CompositionStatus::Unsupported,
                value: expression.into(),
                tex: String::new(),
                steps: Vec::new(),
                operators: operations
                    .iter()
                    .map(|operation: &Operation| operation.signature.operator)
                    .collect(),
                reason: Some(reason),
                arbitrary_constants: Vec::new(),
                held: None,
            }));
        }
    };
    if operations.len() < 2 {
        let structured_operand = (!operations.is_empty())
            .then(|| root_call(&leaf, "组合内层表达式"))
            .transpose()?
            .flatten()
            .filter(|call| STRUCTURED_OPERATOR_NAMES.contains(&call.head.as_str()));
        if let Some(operand) = structured_operand {
            let pending_operators = operations
                .iter()
                .rev()
                .map(|operation| operation.signature.operator)
                .collect();
            let step = operation_step(
                "held-operator-application",
                expression.into(),
                "保留尚未降低的数学对象与外层运算，等待适用的组合规则。",
                tex_code(expression),
            );
            return Ok(Some(CompositionResult {
                status: CompositionStatus::Unresolved,
                value: expression.into(),
                tex: step.tex.clone(),
                steps: vec![step],
                operators: operations
                    .iter()
                    .map(|operation| operation.signature.operator)
                    .collect(),
                reason: Some("组合在语义上有效，但当前没有适用的降低规则".into()),
                arbitrary_constants: Vec::new(),
                held: Some(HeldApplication {
                    source: expression.into(),
                    operand_head: operand.head,
                    pending_operators,
                }),
            }));
        }
        return Ok(None);
    }

    let mut current = leaf;
    let mut steps = Vec::new();
    let mut unresolved = false;
    let mut arbitrary_constants = Vec::new();
    for index in (0..operations.len()).rev() {
        let operation = &operations[index];
        let outcome = apply(engine, operation, &current, verbosity)?;
        current = outcome.value;
        unresolved |= outcome.unresolved;
        arbitrary_constants.extend(outcome.arbitrary_constants);
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
            .map(|operation| operation.signature.operator)
            .collect(),
        reason: unresolved.then(|| "至少一个运算保持未求值".into()),
        arbitrary_constants,
        held: None,
    }))
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
    match operation.signature.value_argument {
        ValueArgument::First => {
            let mut arguments = operation.arguments.clone();
            arguments[value_index] = inner.into();
            format!("{}({})", operation.signature.name, arguments.join(","))
        }
        ValueArgument::Last => format!(
            "{}({})({inner})",
            operation.signature.name,
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
    let head = operation.signature.name;
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

fn collect_operations(
    expression: &str,
    operations: &mut Vec<Operation>,
    depth: usize,
) -> Result<Result<String, String>, EngineError> {
    if depth >= MAX_COMPOSITION_DEPTH {
        return Ok(Err(format!("组合深度超过上限 {MAX_COMPOSITION_DEPTH}")));
    }
    let Some(call) = root_call(expression, "组合表达式")? else {
        return Ok(Ok(expression.into()));
    };
    let Some(signature) = OPERATOR_SIGNATURES
        .iter()
        .find(|signature| signature.name == call.head)
    else {
        return Ok(Ok(expression.into()));
    };
    if !signature.arities.contains(&call.arguments.len()) {
        return Ok(Err(format!(
            "{} 不支持 {} 个参数",
            call.head,
            call.arguments.len()
        )));
    }
    let value_index = value_index(signature, call.arguments.len());
    let Some(inner) = call.arguments.get(value_index).cloned() else {
        return Ok(Err(format!("{} 缺少值参数", call.head)));
    };
    operations.push(Operation {
        signature,
        arguments: call.arguments,
    });
    collect_operations(&inner, operations, depth + 1)
}

fn value_index(signature: &OperatorSignature, argument_count: usize) -> usize {
    match signature.value_argument {
        ValueArgument::First => 0,
        ValueArgument::Last => argument_count.saturating_sub(1),
    }
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
    verbosity: StepVerbosity,
) -> Result<ApplyOutcome, EngineError> {
    let arguments = &operation.arguments;
    match operation.signature.operator {
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
            let steps = if arguments.len() == 4 {
                crate::steps::derive_definite_with_verbosity(
                    engine,
                    current,
                    &arguments[0],
                    &arguments[1],
                    &arguments[2],
                    verbosity,
                )?
            } else {
                derive_integrals_with_verbosity(engine, current, &arguments[0], verbosity)?
            };
            from_steps(steps)
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
            let (kind, variable) = match operation.signature.name {
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
                engine,
                current,
                &arguments[0],
                &arguments[1],
                direction,
                verbosity,
            )?;
            from_steps(steps)
        }
        CompositionOperator::Taylor => {
            let degree = arguments[2]
                .parse::<u32>()
                .map_err(|_| EngineError::InvalidInput("组合 Taylor 次数必须是非负整数".into()))?;
            let result = numeric::taylor(engine, current, &arguments[0], &arguments[1], degree)?;
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
    fn declares_a_small_stable_signature_table() {
        assert!(OPERATOR_SIGNATURES.iter().any(|item| item.name == "D"));
        assert!(OPERATOR_SIGNATURES
            .iter()
            .any(|item| item.name == "Integrate"));
        assert!(OPERATOR_SIGNATURES.iter().any(|item| item.name == "Subst"));
        assert!(OPERATOR_SIGNATURES.iter().any(|item| item.name == "N"));
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
        assert_eq!(result.status, CompositionStatus::Completed);

        let factor = result
            .steps
            .iter()
            .position(|step| step.rule == "compose_factor")
            .unwrap();
        assert!(factor > 0);
        for step in &result.steps[..factor] {
            assert!(step.expr.starts_with("D(x)(Factor("), "{step:#?}");
            assert!(step.tex.contains(r"\operatorname{D}"), "{step:#?}");
            assert!(step.tex.contains(r"\operatorname{Factor}"), "{step:#?}");
        }

        let factor_step = &result.steps[factor];
        assert!(factor_step.expr.starts_with("D(x)("), "{factor_step:#?}");
        assert!(!factor_step.expr.contains("Factor("), "{factor_step:#?}");
        assert!(
            factor_step.tex.contains(r"\operatorname{D}"),
            "{factor_step:#?}"
        );

        for step in &result.steps[factor + 1..] {
            assert!(!step.expr.starts_with("D(x)("), "{step:#?}");
            assert!(!step.tex.contains(r"\operatorname{Factor}"), "{step:#?}");
        }
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
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-4)", result.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
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
            ("Integrate(x)Apart((x+1)/(x^2-1),x)", "Ln(x-1)"),
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
            assert!(result
                .steps
                .iter()
                .any(|step| step.rule == "compose_algebra_transform"));
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
            .position(|step| step.why.contains("外层求导"))
            .unwrap();
        assert!(limit_result < derivative);
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
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "compose_taylor"));
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
    fn single_operation_keeps_existing_fast_path() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(
            execute_steps(&mut engine, "D(x)x^2", StepVerbosity::Concise)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn structured_operands_remain_held_when_no_lowering_rule_applies() {
        let mut engine = RustEngine::spawn().unwrap();
        for expression in [
            "D(x)Solve({x==1},{x})",
            "Factor(DoubleIntegral(x+y,y,0,x,x,0,1))",
            "N(MatrixSolve({{1,0},{0,1}},{1,2}),10)",
            "D(x)OdeSolveNumeric(y'==y,x,y,0,1,2)",
            "D(x)Plot(Sin(x),x,-1,1)",
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
}
