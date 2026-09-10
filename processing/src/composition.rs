//! Bounded, inside-out dispatch for teaching chains made from a small set of
//! product operations. This is an execution protocol, not a second CAS AST.

use serde::Serialize;

use crate::algebra::{self, TransformKind};
use crate::engine::{Engine, EngineError};
use crate::input::{root_call, strip_tex_delimiters, validate_expression, RootCall};
use crate::numeric;
use crate::steps::{
    derive_integrals_with_verbosity, derive_steps_order_with_verbosity, Step, StepImportance,
    StepVerbosity,
};

const MAX_COMPOSITION_DEPTH: usize = 16;
const DEFAULT_PRECISION: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionOperator {
    Derivative,
    Factor,
    Integral,
    Substitute,
    Approximate,
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
const INTEGRATE_ARITIES: &[usize] = &[2];
const SUBST_ARITIES: &[usize] = &[3];
const APPROXIMATE_ARITIES: &[usize] = &[1, 2];

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
        .is_some_and(|head| OPERATOR_SIGNATURES.iter().any(|item| item.name == head))
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
            }));
        }
    };
    if operations.len() < 2 {
        return Ok(None);
    }

    let mut current = leaf;
    let mut steps = Vec::new();
    let mut unresolved = false;
    for operation in operations.iter().rev() {
        let outcome = apply(engine, operation, &current, verbosity)?;
        current = outcome.value;
        unresolved |= outcome.unresolved;
        steps.extend(outcome.steps);
    }
    let tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
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
    }))
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
            let steps = derive_integrals_with_verbosity(engine, current, &arguments[0], verbosity)?;
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
            })
        }
    }
}

fn from_steps(steps: Vec<Step>) -> Result<ApplyOutcome, EngineError> {
    let value = steps
        .last()
        .map(|step| step.expr.clone())
        .ok_or_else(|| EngineError::Parse("组合运算没有产生最终步骤".into()))?;
    let unresolved = value.starts_with("Integrate(") || value.starts_with("D(");
    Ok(ApplyOutcome {
        value,
        steps,
        unresolved,
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
}
