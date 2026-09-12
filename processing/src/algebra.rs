//! Stable processing API for common algebraic transformations.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression, validate_symbol};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta, OperatorId,
    RepresentationPreference, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticOperation, SemanticState,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformKind {
    Simplify,
    Tidy,
    Expand,
    Factor,
    Apart,
}

impl TransformKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Simplify => "Simplify",
            Self::Tidy => "Tidy",
            Self::Expand => "Expand",
            Self::Factor => "Factor",
            Self::Apart => "Apart",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformRequest {
    pub kind: TransformKind,
    pub variable: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TransformOperation;

impl SemanticOperation<TransformRequest> for TransformOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &TransformRequest,
    ) -> Result<Computation, EngineError> {
        let capability = match request.kind {
            TransformKind::Factor => ObjectCapability::Factor,
            TransformKind::Expand => ObjectCapability::Expand,
            TransformKind::Simplify | TransformKind::Tidy => ObjectCapability::Simplify,
            TransformKind::Apart => ObjectCapability::Simplify,
        };
        if !input.semantics.capabilities.contains(capability) {
            return Err(EngineError::InvalidInput(format!(
                "该数学对象不具备 {} 能力",
                request.kind.name()
            )));
        }
        let result = transform(
            engine,
            &input.print_source(),
            request.kind,
            request.variable.as_deref(),
        )?;
        let metadata = if result.unresolved {
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
        } else {
            ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
        };
        let mut semantics = SemanticState {
            kind: if result.unresolved {
                ValueKind::Unevaluated
            } else {
                crate::semantic::analyze_input(&result.output, "代数变换结果")?
                    .semantic
                    .kind
            },
            interpretation: if result.unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: request.kind.name().into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata,
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let parsed = object_from_source(input.id, &result.output, semantics.clone())?;
        if result.unresolved {
            crate::semantic_core::promote_held_application(
                request.kind.name(),
                &parsed.raw_expression(),
                &mut semantics,
            )?;
        }
        let normalization = NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: Vec::new(),
            mode: NormalizationMode::Operation(match request.kind {
                TransformKind::Factor => OperatorId::Factor,
                _ => OperatorId::AlgebraTransform,
            }),
        };
        let mut output = input.clone();
        if result.unresolved {
            output.apply(ObjectDelta {
                expression: Some(parsed.raw_expression()),
                semantics: Some(semantics),
                overlay: None,
                normalization: None,
            });
        } else {
            let representation = output
                .retain_representation(
                    parsed.raw_expression(),
                    RepresentationPreference::Named(request.kind.name().into()),
                    Some(normalization),
                )
                .expect("stable representation budget always retains the newest candidate");
            output.activate_representation(representation)?;
        }
        let event = RuleEvent {
            rule: if result.unresolved {
                "hold-algebra-transform"
            } else if result.changed {
                "apply-algebra-transform"
            } else {
                "confirm-algebra-normal-form"
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![("operation".into(), request.kind.name().into())],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: output.print_source(),
                explanation: if result.unresolved {
                    "保留当前无法完成的代数变换。"
                } else {
                    "应用代数变换并规范化结果。"
                }
                .into(),
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
            certificates: (!result.unresolved)
                .then(|| crate::semantic_core::Certificate {
                    kind: "equivalent-representation".into(),
                    payload: format!(
                        "operation={};before={};after={}",
                        request.kind.name(),
                        result.input,
                        result.output
                    ),
                })
                .into_iter()
                .collect(),
            effects: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransformResult {
    pub operation: String,
    pub input: String,
    pub output: String,
    pub tex: String,
    /// Whether the output tree differs from the engine's evaluated input tree.
    pub changed: bool,
    /// The operation remained as the result head and was not completed.
    pub unresolved: bool,
}

pub fn transform(
    engine: &mut dyn Engine,
    input: &str,
    kind: TransformKind,
    variable: Option<&str>,
) -> Result<TransformResult, EngineError> {
    validate_expression(input, "表达式")?;
    let operation = kind.name();
    let command = match kind {
        TransformKind::Apart => {
            let variable =
                variable.ok_or_else(|| EngineError::InvalidInput("Apart 需要指定变量".into()))?;
            validate_symbol(variable, "变量")?;
            format!("Apart({input},{variable})")
        }
        _ => {
            if variable.is_some() {
                return Err(EngineError::InvalidInput(format!(
                    "{operation} 不接受变量参数"
                )));
            }
            format!("{operation}({input})")
        }
    };

    // Evaluate the input independently so `changed` describes the requested
    // transformation rather than ordinary parsing/canonicalization.
    let before = engine.eval_expr(input)?;
    let result = engine.eval(&command)?;
    // `FWatom` is an internal factorization worker wrapper, not a
    // mathematical result. Keep the requested transform held instead of
    // leaking this implementation detail into later operations.
    let unresolved = matches!(&result.expr, Expr::Call { head, .. }
        if head == operation || head == "FWatom");
    let (output, tex) = if unresolved {
        let input_tex = strip_tex_delimiters(&engine.eval(input)?.tex);
        (
            command.clone(),
            format!(r"\operatorname{{{operation}}}\left({input_tex}\right)"),
        )
    } else {
        (result.expr.to_string(), strip_tex_delimiters(&result.tex))
    };
    Ok(TransformResult {
        operation: operation.into(),
        input: input.trim().into(),
        output,
        tex,
        changed: result.expr != before,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{ObjectId, SemanticState};

    fn equivalent(engine: &mut dyn Engine, left: &str, right: &str) {
        let result = engine
            .eval(&format!("Simplify(({left})-({right}))"))
            .unwrap();
        assert_eq!(result.expr.to_string(), "0", "{left} != {right}");
    }

    #[test]
    fn common_polynomial_transforms_are_structured_and_equivalent() {
        let mut engine = RustEngine::spawn().unwrap();
        for (kind, input, expected, variable) in [
            (TransformKind::Simplify, "x+x", "2*x", None),
            (TransformKind::Tidy, "(x+x)/2", "x", None),
            (TransformKind::Expand, "(x+1)^2", "x^2+2*x+1", None),
            (TransformKind::Factor, "x^2-1", "(x-1)*(x+1)", None),
            (
                TransformKind::Apart,
                "1/(x^2-1)",
                "1/(2*(x-1))-1/(2*(x+1))",
                Some("x"),
            ),
            (TransformKind::Apart, "(x+1)/(x^2-1)", "1/(x-1)", Some("x")),
        ] {
            let result = transform(&mut engine, input, kind, variable).unwrap();
            assert!(
                !result.unresolved,
                "{}: {}",
                result.operation, result.output
            );
            assert!(!result.tex.is_empty());
            equivalent(&mut engine, &result.output, expected);
        }
    }

    #[test]
    fn unchanged_and_unresolved_are_distinct() {
        let mut engine = RustEngine::spawn().unwrap();
        let unchanged = transform(&mut engine, "x", TransformKind::Simplify, None).unwrap();
        assert!(!unchanged.changed);
        assert!(!unchanged.unresolved);

        let unsupported = transform(&mut engine, "Sin(x)", TransformKind::Factor, None).unwrap();
        assert!(unsupported.unresolved);
        assert!(unsupported.output.starts_with("Factor("));

        let internal_wrapper =
            transform(&mut engine, "x^4/2+x^2+C", TransformKind::Factor, None).unwrap();
        assert!(internal_wrapper.unresolved);
        assert_eq!(internal_wrapper.output, "Factor(x^4/2+x^2+C)");
        assert!(!internal_wrapper.output.contains("FWatom"));
    }

    #[test]
    fn rejects_bad_inputs_and_operation_arguments() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(transform(&mut engine, "x);Echo(1);(x", TransformKind::Simplify, None).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Apart, None).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Apart, Some("x;Echo(1)")).is_err());
        assert!(transform(&mut engine, "x^2", TransformKind::Expand, Some("x")).is_err());
    }

    #[test]
    fn object_transform_preserves_identity_and_distinguishes_held_results() {
        let input = |source: &str| {
            object_from_source(
                ObjectId(31),
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
        let solved = TransformOperation
            .compute(
                &mut engine,
                &input("(x+1)^2"),
                &TransformRequest {
                    kind: TransformKind::Expand,
                    variable: None,
                },
            )
            .unwrap();
        let output = solved.value().unwrap();
        assert_eq!(output.id, ObjectId(31));
        assert_eq!(output.revision.0, 1);
        assert_eq!(output.print_source(), "x^2+2*x+1");
        assert!(output.meets_normalization(NormalizationLevel::Domain));
        assert_eq!(output.stable_representation_count(), 2);
        assert_eq!(
            output.representation_preference(output.active_representation()),
            Some(&RepresentationPreference::Named("Expand".into()))
        );
        assert_eq!(solved.certificates.len(), 1);
        let principal = output
            .operation_session(Some(crate::semantic_core::RepresentationId(0)))
            .unwrap();
        crate::input::with_parse_env(|env| {
            assert_eq!(principal.view(env).print_source(), "(x+1)^2");
        });

        let apart = TransformOperation
            .compute(
                &mut engine,
                &input("1/(x^2-1)"),
                &TransformRequest {
                    kind: TransformKind::Apart,
                    variable: Some("x".into()),
                },
            )
            .unwrap();
        assert_eq!(apart.value().unwrap().stable_representation_count(), 2);
        equivalent(
            &mut engine,
            &apart.value().unwrap().print_source(),
            "1/(2*(x-1))-1/(2*(x+1))",
        );

        let held = TransformOperation
            .compute(
                &mut engine,
                &input("Sin(x)"),
                &TransformRequest {
                    kind: TransformKind::Factor,
                    variable: None,
                },
            )
            .unwrap();
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        assert!(matches!(
            held.subject().unwrap().semantics.interpretation,
            SemanticInterpretation::HeldTypedApplication(_)
        ));
    }

    #[test]
    fn equivalent_transforms_switch_between_retained_asts_without_changing_identity() {
        let input = object_from_source(
            ObjectId(32),
            "(x+1)^2",
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let identity = input.identity();
        let mut engine = RustEngine::spawn().unwrap();
        let expanded = TransformOperation
            .compute(
                &mut engine,
                &input,
                &TransformRequest {
                    kind: TransformKind::Expand,
                    variable: None,
                },
            )
            .unwrap();
        let factored = TransformOperation
            .compute(
                &mut engine,
                expanded.value().unwrap(),
                &TransformRequest {
                    kind: TransformKind::Factor,
                    variable: None,
                },
            )
            .unwrap();
        let output = factored.value().unwrap();

        assert_eq!(output.identity(), identity);
        assert_eq!(
            output.active_representation(),
            crate::semantic_core::RepresentationId(0)
        );
        assert_eq!(output.stable_representation_count(), 2);
        assert_eq!(output.print_source(), "(x+1)^2");
        assert_eq!(output.revision.0, 2);
        assert_eq!(factored.certificates.len(), 1);
    }
}
