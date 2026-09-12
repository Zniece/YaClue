//! Shared numerical evaluation services for product-level algorithms.

use crate::engine::{Engine, EngineError, EvalResult, Expr};
use crate::input::{strip_tex_delimiters, validate_expression, validate_symbol};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
#[cfg(test)]
use crate::semantic_core::object_from_source;
use crate::semantic_core::{
    CapabilitySet, Certificate, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta, OperatorId, RuleEvent,
    RuleImportance, RulePayload, RulePresentation, RuleTrace, SemanticInterpretation,
    SemanticOperation, SemanticState,
};
use serde::Serialize;

/// Product API limit. The core supports a larger technical ceiling, but an
/// interactive request at extreme precision is not a useful default contract.
pub const MAX_PRECISION_DIGITS: u32 = 1_000;
pub const MAX_TAYLOR_DEGREE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericKind {
    ExactReal,
    ApproximateReal,
    Complex,
    NonFinite,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericResult {
    pub output: String,
    pub tex: String,
    pub precision_digits: u32,
    pub kind: NumericKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericEvaluationRequest {
    pub precision_digits: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NumericEvaluationOperation;

impl SemanticOperation<NumericEvaluationRequest> for NumericEvaluationOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &NumericEvaluationRequest,
    ) -> Result<Computation, EngineError> {
        validate_precision(request.precision_digits)?;
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::NumericEvaluate)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备数值计算能力".into(),
            ));
        }
        let result = approximate(engine, &input.print_source(), request.precision_digits)?;
        let unresolved = result.kind == NumericKind::Unresolved;
        let no_value = result.kind == NumericKind::NonFinite && result.output == "Undefined";
        let output_source = if unresolved {
            format!("N({},{})", input.print_source(), request.precision_digits)
        } else {
            result.output.clone()
        };
        let mut semantics = SemanticState {
            kind: if unresolved || no_value {
                ValueKind::Unevaluated
            } else {
                crate::semantic::analyze_input(&output_source, "数值计算结果")?
                    .semantic
                    .kind
            },
            interpretation: if unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: "N".into(),
                }
            } else if no_value {
                SemanticInterpretation::StructuredUnevaluated {
                    reason: "numeric result is undefined".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: if unresolved {
                ResultMetadata::unresolved(
                    Exactness::Approximate,
                    OutcomeReason::AlgorithmUncovered,
                )
            } else if no_value {
                ResultMetadata::no_result(
                    Exactness::Approximate,
                    OutcomeReason::MathematicalAbsence,
                )
            } else {
                ResultMetadata::solved(
                    if result.kind == NumericKind::ExactReal {
                        Exactness::Exact
                    } else {
                        Exactness::Approximate
                    },
                    ConditionSet::empty(),
                )
            },
            capabilities: if no_value {
                CapabilitySet::empty()
            } else {
                CapabilitySet::symbolic_expression()
            },
            requirements: Vec::new(),
        };
        let parsed = crate::semantic_core::parse_engine_expression(&output_source)?;
        if unresolved {
            crate::semantic_core::promote_held_application(
                "N",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!unresolved && !no_value).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Approximate),
            }),
        });
        let event = RuleEvent {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: if unresolved {
                "hold-numeric-evaluation"
            } else if no_value {
                "numeric-evaluation-undefined"
            } else {
                "numeric-evaluation"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![("precision".into(), request.precision_digits.to_string())],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            transformation: None,
            presentation: (!unresolved).then(|| RulePresentation {
                expression: output.print_source(),
                explanation: if no_value {
                    "数值计算没有定义。"
                } else {
                    "按指定有效数字计算数值近似。"
                }
                .into(),
                tex_override: Some(result.tex),
            }),
        };
        Ok(Computation {
            output: if unresolved {
                ComputationOutput::Held(output)
            } else if no_value {
                ComputationOutput::NoValue(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootStatus {
    Converged,
    NoConvergence,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct RootResult {
    pub status: RootStatus,
    pub output: String,
    pub tex: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FindRootRequest {
    pub variable: String,
    pub initial: f64,
    pub tolerance: f64,
    pub bracket: Option<(f64, f64)>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FindRootOperation;

impl SemanticOperation<FindRootRequest> for FindRootOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &FindRootRequest,
    ) -> Result<Computation, EngineError> {
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::FindNumericRoot)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备数值求根能力".into(),
            ));
        }
        let result = find_root(
            engine,
            &input.print_source(),
            &request.variable,
            request.initial,
            request.tolerance,
            request.bracket,
        )?;
        let converged = result.status == RootStatus::Converged;
        let output_source = if converged {
            result.output.clone()
        } else {
            format!(
                "FindRoot({},{},{})",
                input.print_source(),
                request.variable,
                request.initial
            )
        };
        let mut semantics = if converged {
            SemanticState {
                kind: crate::semantic::analyze_input(&output_source, "数值根")?
                    .semantic
                    .kind,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::solved(Exactness::Approximate, ConditionSet::empty()),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            }
        } else {
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::HeldApplication {
                    operator: "FindRoot".into(),
                },
                metadata: ResultMetadata::unresolved(
                    Exactness::Approximate,
                    OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            }
        };
        let parsed = crate::semantic_core::parse_engine_expression(&output_source)?;
        if !converged {
            crate::semantic_core::promote_held_application(
                "FindRoot",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: converged.then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::FindRoot),
            }),
        });
        let event = RuleEvent {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: if converged {
                "numeric-root"
            } else {
                "hold-numeric-root"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![
                ("variable".into(), request.variable.clone()),
                ("initial".into(), request.initial.to_string()),
                ("tolerance".into(), request.tolerance.to_string()),
            ],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            transformation: None,
            presentation: converged.then(|| RulePresentation {
                expression: result.output,
                explanation: "从给定初值求得数值根。".into(),
                tex_override: Some(result.tex),
            }),
        };
        Ok(Computation {
            output: if converged {
                ComputationOutput::Value(output)
            } else {
                ComputationOutput::Held(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: vec![Certificate {
                kind: "numeric_root_attempt".into(),
                payload: format!(
                    "status={:?}; initial={}; tolerance={}; bracket={:?}",
                    result.status, request.initial, request.tolerance, request.bracket
                ),
            }],
            effects: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaylorResult {
    pub output: String,
    pub tex: String,
    pub degree: u32,
    pub unresolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaylorRequest {
    pub variable: String,
    pub point: String,
    pub degree: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TaylorOperation;

impl SemanticOperation<TaylorRequest> for TaylorOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &TaylorRequest,
    ) -> Result<Computation, EngineError> {
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::ExpandTaylor)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备 Taylor 展开能力".into(),
            ));
        }
        let result = taylor(
            engine,
            &input.print_source(),
            &request.variable,
            &request.point,
            request.degree,
        )?;
        let output_source = if result.unresolved {
            format!(
                "Taylor({},{},{})({})",
                request.variable,
                request.point,
                request.degree,
                input.print_source()
            )
        } else {
            result.output.clone()
        };
        let mut semantics = SemanticState {
            kind: if result.unresolved {
                ValueKind::Unevaluated
            } else {
                crate::semantic::analyze_input(&output_source, "Taylor 展开结果")?
                    .semantic
                    .kind
            },
            interpretation: if result.unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: "Taylor".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: if result.unresolved {
                ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
            } else {
                ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
            },
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let parsed = crate::semantic_core::parse_engine_expression(&output_source)?;
        if result.unresolved {
            crate::semantic_core::promote_held_application(
                "Taylor",
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!result.unresolved).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Taylor),
            }),
        });
        let event = RuleEvent {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: if result.unresolved {
                "hold-taylor"
            } else {
                "taylor-expand"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![
                ("variable".into(), request.variable.clone()),
                ("point".into(), request.point.clone()),
                ("degree".into(), request.degree.to_string()),
            ],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            transformation: None,
            presentation: (!result.unresolved).then(|| RulePresentation {
                expression: output.print_source(),
                explanation: "在指定点展开 Taylor 多项式。".into(),
                tex_override: Some(result.tex),
            }),
        };
        Ok(Computation {
            output: if result.unresolved {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

pub fn approximate(
    engine: &mut dyn Engine,
    expression: &str,
    precision_digits: u32,
) -> Result<NumericResult, EngineError> {
    validate_expression(expression, "数值表达式")?;
    validate_precision(precision_digits)?;
    let result = engine.eval(&format!("N({expression},{precision_digits})"))?;
    Ok(NumericResult {
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        precision_digits,
        kind: numeric_kind(&result.expr),
    })
}

pub fn find_root(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    initial: f64,
    accuracy: f64,
    bounds: Option<(f64, f64)>,
) -> Result<RootResult, EngineError> {
    validate_expression(expression, "求根表达式")?;
    validate_symbol(variable, "求根变量")?;
    if !initial.is_finite() || !accuracy.is_finite() || accuracy <= 0.0 {
        return Err(EngineError::InvalidInput(
            "初值必须有限，精度必须为有限正数".into(),
        ));
    }
    let command = if let Some((min, max)) = bounds {
        if !min.is_finite() || !max.is_finite() || min >= max || initial <= min || initial >= max {
            return Err(EngineError::InvalidInput(
                "求根区间必须有限且递增，初值必须位于区间内部".into(),
            ));
        }
        format!("Newton({expression},{variable},{initial},{accuracy},{min},{max})")
    } else {
        format!("Newton({expression},{variable},{initial},{accuracy})")
    };
    let result = engine.eval(&command)?;
    let status = match &result.expr {
        Expr::Symbol(value) if value == "Fail" => RootStatus::NoConvergence,
        expr if matches!(
            numeric_kind(expr),
            NumericKind::ExactReal | NumericKind::ApproximateReal | NumericKind::Complex
        ) =>
        {
            RootStatus::Converged
        }
        _ => RootStatus::Unresolved,
    };
    Ok(RootResult {
        status,
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
    })
}

pub fn taylor(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    point: &str,
    degree: u32,
) -> Result<TaylorResult, EngineError> {
    validate_expression(expression, "级数表达式")?;
    validate_expression(point, "展开点")?;
    validate_symbol(variable, "展开变量")?;
    if degree > MAX_TAYLOR_DEGREE {
        return Err(EngineError::InvalidInput(format!(
            "Taylor 阶数不能超过 {MAX_TAYLOR_DEGREE}"
        )));
    }
    let result = engine.eval(&format!(
        "Taylor({variable},{point},{degree})({expression})"
    ))?;
    let unresolved = matches!(&result.expr, Expr::Call { head, .. } if head == "Taylor");
    Ok(TaylorResult {
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        degree,
        unresolved,
    })
}

fn validate_precision(precision_digits: u32) -> Result<(), EngineError> {
    if (1..=MAX_PRECISION_DIGITS).contains(&precision_digits) {
        Ok(())
    } else {
        Err(EngineError::InvalidInput(format!(
            "数值精度必须在 1..={MAX_PRECISION_DIGITS} 位之间"
        )))
    }
}

fn numeric_kind(expr: &Expr) -> NumericKind {
    match expr {
        Expr::Number(value) if value.contains(['.', 'e', 'E']) => NumericKind::ApproximateReal,
        Expr::Number(_) => NumericKind::ExactReal,
        Expr::Symbol(value) if matches!(value.as_str(), "Infinity" | "Undefined") => {
            NumericKind::NonFinite
        }
        Expr::Call { head, args }
            if head == "-"
                && args.len() == 1
                && numeric_kind(&args[0]) == NumericKind::NonFinite =>
        {
            NumericKind::NonFinite
        }
        Expr::Call { head, args } if head == "Complex" && args.len() == 2 => {
            if args
                .iter()
                .any(|value| numeric_kind(value) == NumericKind::NonFinite)
            {
                NumericKind::NonFinite
            } else if args.iter().all(|value| {
                matches!(
                    numeric_kind(value),
                    NumericKind::ExactReal | NumericKind::ApproximateReal
                )
            }) {
                NumericKind::Complex
            } else {
                NumericKind::Unresolved
            }
        }
        _ => NumericKind::Unresolved,
    }
}

/// Evaluate `expression(variable = value)` for all values in bounded batches.
///
/// Each batch uses one engine request. Non-finite or unresolved scalar results
/// are represented as `NaN`, allowing callers to apply domain-specific gap or
/// convergence policies without duplicating engine protocol code.
pub fn evaluate_real_batch(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    values: &[f64],
    batch_size: usize,
) -> Result<Vec<f64>, EngineError> {
    let mut output = Vec::with_capacity(values.len());
    for chunk in values.chunks(batch_size.max(1)) {
        let items: Vec<String> = chunk
            .iter()
            .map(|value| {
                format!("N(Eval(ApplyPure(\"Subst\", {{{variable},{value},{expression}}})))")
            })
            .collect();
        let command = format!("N({{{}}})", items.join(", "));
        let result: EvalResult = engine.eval(&command)?;
        let batch = flatten_number_list(&result.expr);
        if batch.len() != chunk.len() {
            return Err(EngineError::Eval(format!(
                "batch evaluation returned {} values, expected {}",
                batch.len(),
                chunk.len()
            )));
        }
        output.extend(batch);
    }
    Ok(output)
}

fn flatten_number_list(expr: &Expr) -> Vec<f64> {
    fn scalar(expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Number(value) => value.parse::<f64>().ok(),
            _ => None,
        }
    }
    match expr {
        Expr::Call { head, args } if head == "List" => args
            .iter()
            .map(|expr| scalar(expr).unwrap_or(f64::NAN))
            .collect(),
        Expr::Call { head, args } if head == "N" => {
            args.first().map(flatten_number_list).unwrap_or_default()
        }
        one => scalar(one).into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{ObjectId, SemanticState};

    #[test]
    fn batched_evaluation_matches_direct_values_and_preserves_gaps() {
        let mut engine = RustEngine::spawn().expect("boot");
        let xs: Vec<f64> = (0..16).map(|i| 0.1 * i as f64).collect();
        let ys = evaluate_real_batch(&mut engine, "Sin(x)*Exp(-x)", "x", &xs, 8).expect("batch");
        for (x, y) in xs.iter().zip(&ys) {
            let expected = x.sin() * (-x).exp();
            assert!((y - expected).abs() < 1e-9, "at x={x}");
        }

        let with_pole = evaluate_real_batch(&mut engine, "1/x", "x", &[-1.0, 0.0, 1.0], 3)
            .expect("batch with pole");
        assert_eq!(with_pole[0], -1.0);
        assert!(with_pole[1].is_nan());
        assert_eq!(with_pole[2], 1.0);
    }

    #[test]
    fn classifies_public_numeric_results() {
        let mut engine = RustEngine::spawn().expect("boot");
        for (expression, expected) in [
            ("1/2", NumericKind::ApproximateReal),
            ("1", NumericKind::ExactReal),
            ("Ln(-2)", NumericKind::Complex),
            ("1/0", NumericKind::NonFinite),
            ("-Infinity", NumericKind::NonFinite),
            ("x+1/2", NumericKind::Unresolved),
        ] {
            assert_eq!(
                approximate(&mut engine, expression, 20).unwrap().kind,
                expected
            );
        }
    }

    #[test]
    fn finds_roots_and_builds_taylor_polynomials() {
        let mut engine = RustEngine::spawn().expect("boot");
        let root = find_root(&mut engine, "Sin(x)", "x", 3.0, 1e-12, Some((2.0, 4.0))).unwrap();
        assert_eq!(root.status, RootStatus::Converged);
        let value: f64 = root.output.parse().unwrap();
        assert!((value - std::f64::consts::PI).abs() < 1e-9);

        let failed = find_root(&mut engine, "x^2+1", "x", 1.0, 1e-10, Some((0.0, 2.0))).unwrap();
        assert_eq!(failed.status, RootStatus::NoConvergence);

        let series = taylor(&mut engine, "Exp(x)", "x", "0", 5).unwrap();
        assert!(!series.unresolved);
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify(({})-(1+x+x^2/2+x^3/6+x^4/24+x^5/120))",
                    series.output
                ))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn rejects_invalid_public_numeric_requests() {
        let mut engine = RustEngine::spawn().expect("boot");
        assert!(approximate(&mut engine, "Pi", 0).is_err());
        assert!(approximate(&mut engine, "Pi", MAX_PRECISION_DIGITS + 1).is_err());
        assert!(find_root(&mut engine, "x", "x", 0.0, 0.0, None).is_err());
        assert!(find_root(&mut engine, "x", "x", 0.0, 1e-6, Some((1.0, -1.0))).is_err());
        assert!(taylor(&mut engine, "Exp(x)", "x", "0", MAX_TAYLOR_DEGREE + 1).is_err());
        assert!(taylor(&mut engine, "x);Echo(1);(x", "x", "0", 2).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }

    #[test]
    fn object_numeric_evaluation_separates_values_held_and_no_value() {
        let object = |source: &str| {
            object_from_source(
                ObjectId(51),
                source,
                SemanticState {
                    kind: ValueKind::Expression,
                    interpretation: SemanticInterpretation::PlainExpression,
                    metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    capabilities: CapabilitySet::symbolic_expression(),
                    requirements: Vec::new(),
                },
            )
            .unwrap()
        };
        let mut engine = RustEngine::spawn().unwrap();
        let request = NumericEvaluationRequest {
            precision_digits: 20,
        };
        let value = NumericEvaluationOperation
            .compute(&mut engine, &object("Pi"), &request)
            .unwrap();
        assert_eq!(value.value().unwrap().id, ObjectId(51));
        assert_eq!(
            value.value().unwrap().semantics.metadata.exactness,
            Exactness::Approximate
        );
        assert!(value
            .value()
            .unwrap()
            .meets_normalization(NormalizationLevel::Domain));

        let held = NumericEvaluationOperation
            .compute(&mut engine, &object("x+1/2"), &request)
            .unwrap();
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        assert!(held.subject().unwrap().print_source().starts_with("N("));

        let absent = NumericEvaluationOperation
            .compute(&mut engine, &object("Undefined"), &request)
            .unwrap();
        assert!(matches!(absent.output, ComputationOutput::NoValue(_)));
    }

    #[test]
    fn object_taylor_preserves_identity_and_holds_uncovered_series() {
        let object = |source: &str| {
            object_from_source(
                ObjectId(73),
                source,
                SemanticState {
                    kind: ValueKind::Expression,
                    interpretation: SemanticInterpretation::PlainExpression,
                    metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    capabilities: CapabilitySet::symbolic_expression(),
                    requirements: Vec::new(),
                },
            )
            .unwrap()
        };
        let request = TaylorRequest {
            variable: "x".into(),
            point: "0".into(),
            degree: 4,
        };
        let mut engine = RustEngine::spawn().unwrap();
        let value = TaylorOperation
            .compute(&mut engine, &object("Exp(x)"), &request)
            .unwrap();
        assert_eq!(value.value().unwrap().id, ObjectId(73));
        assert!(value.value().unwrap().revision.0 > 0);
        assert!(value
            .value()
            .unwrap()
            .meets_normalization(NormalizationLevel::Domain));
        assert_eq!(value.trace.unwrap().events[0].rule, "taylor-expand");

        struct HeldTaylorEngine;
        impl Engine for HeldTaylorEngine {
            fn eval(&mut self, _command: &str) -> Result<EvalResult, EngineError> {
                Ok(EvalResult {
                    expr: Expr::Call {
                        head: "Taylor".into(),
                        args: vec![],
                    },
                    tex: "Taylor".into(),
                })
            }
        }
        let held = TaylorOperation
            .compute(&mut HeldTaylorEngine, &object("Sin(1/x)"), &request)
            .unwrap();
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        assert!(held
            .subject()
            .unwrap()
            .print_source()
            .starts_with("Taylor("));
        assert!(held.subject().unwrap().print_source().contains("Sin(1/x)"));
    }

    #[test]
    fn object_find_root_returns_an_approximate_value_and_holds_failed_attempts() {
        let object = |source: &str| {
            object_from_source(
                ObjectId(89),
                source,
                SemanticState {
                    kind: ValueKind::Expression,
                    interpretation: SemanticInterpretation::PlainExpression,
                    metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    capabilities: CapabilitySet::symbolic_expression(),
                    requirements: Vec::new(),
                },
            )
            .unwrap()
        };
        let mut engine = RustEngine::spawn().unwrap();
        let solved = FindRootOperation
            .compute(
                &mut engine,
                &object("x^2-2"),
                &FindRootRequest {
                    variable: "x".into(),
                    initial: 1.0,
                    tolerance: 1e-10,
                    bracket: None,
                },
            )
            .unwrap();
        assert!(matches!(solved.output, ComputationOutput::Value(_)));
        assert_eq!(solved.value().unwrap().id, ObjectId(89));
        assert_eq!(
            solved.value().unwrap().semantics.metadata.exactness,
            Exactness::Approximate
        );
        assert!(solved
            .value()
            .unwrap()
            .semantics
            .capabilities
            .contains(ObjectCapability::Differentiate));

        let failed = FindRootOperation
            .compute(
                &mut engine,
                &object("x^2+1"),
                &FindRootRequest {
                    variable: "x".into(),
                    initial: 1.0,
                    tolerance: 1e-10,
                    bracket: Some((0.0, 2.0)),
                },
            )
            .unwrap();
        assert!(matches!(failed.output, ComputationOutput::Held(_)));
        assert_eq!(
            failed.subject().unwrap().semantics.metadata.resolution,
            crate::protocol::ResolutionState::Unresolved
        );
        assert!(failed
            .subject()
            .unwrap()
            .print_source()
            .starts_with("FindRoot("));
        assert_eq!(failed.certificates[0].kind, "numeric_root_attempt");
    }
}
