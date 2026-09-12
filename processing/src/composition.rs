//! Bounded, inside-out dispatch for teaching chains made from a small set of
//! product operations. This is an execution protocol, not a second CAS AST.

use serde::Serialize;

use crate::engine::{Engine, EngineError};
use crate::input::{strip_tex_delimiters, RootCall};
use crate::protocol::{ConditionSet, ResultMetadata};
use crate::semantic_core::{
    is_known_operator, operator_descriptor, ComputationOutput, ObjectCapability,
    OperatorDescriptor, SemanticInterpretation,
};
pub use crate::semantic_core::{OperatorId as CompositionOperator, ValueArgument};
use crate::steps::{Step, StepVerbosity};

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
    pub sampled_data: Option<crate::semantic_core::SampledTrajectory>,
    pub plot: Option<crate::plot::PlotEffect>,
    pub effect_only: bool,
    pub analysis: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeldApplication {
    pub source: String,
    pub operand_head: String,
    pub pending_operators: Vec<CompositionOperator>,
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
        .is_some_and(is_known_operator)
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
    let root_is_effect = matches!(
        input.root.form,
        crate::elaboration::MathematicalForm::EffectApplication { .. }
    );
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
    if root_is_effect {
        let computation = crate::arithmetic::execute_elaborated_structure(engine, &input.root)?;
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
        let plot = computation
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                crate::semantic_core::Effect::Plot(plot) => Some(plot),
                crate::semantic_core::Effect::Ui(_) => None,
            });
        let value = plot
            .as_ref()
            .map(|effect| effect.expression.clone())
            .unwrap_or_else(|| input.root.object.print_source());
        let tex = strip_tex_delimiters(&engine.eval(&value)?.tex);
        return Ok(Some(CompositionResult {
            status: CompositionStatus::Completed,
            value,
            tex,
            steps,
            operators: vec![CompositionOperator::Plot],
            reason: None,
            arbitrary_constants: Vec::new(),
            held: None,
            conditions: ConditionSet::empty(),
            outcome: None,
            sampled_data: None,
            plot,
            effect_only: true,
            analysis: None,
        }));
    }
    if structural_context || root_is_native {
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
        let analysis = computation.certificates.iter().find_map(|certificate| {
            matches!(
                certificate.kind.as_str(),
                "extrema_analysis" | "lagrange_analysis"
            )
            .then(|| serde_json::from_str(&certificate.payload).ok())
            .flatten()
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
            sampled_data: match &subject.semantics.interpretation {
                SemanticInterpretation::NumericTrajectory(trajectory) => Some(trajectory.clone()),
                _ => None,
            },
            plot: None,
            effect_only: false,
            analysis,
        }));
    }
    Ok(None)
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
    fn defined_integrals_compose_without_principal_value_fallback() {
        let mut engine = RustEngine::spawn().unwrap();
        let gamma = crate::elaboration::elaborate_input(
            "D(x)ImproperIntegral(t^(x-1)*Exp(-t),t,0,Infinity)",
        )
        .unwrap();
        let result = execute_elaborated(&mut engine, &gamma, StepVerbosity::Detailed, true)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(result.value, "Gamma(x)*PolyGamma(0,x)");
        assert!(result
            .operators
            .contains(&CompositionOperator::ImproperIntegral));
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "intrinsic-gamma-lowering"));

        let ordinary =
            crate::elaboration::elaborate_input("D(x)ImproperIntegral(1/t,t,-1,1,{0})").unwrap();
        let result = execute_elaborated(&mut engine, &ordinary, StepVerbosity::Detailed, false)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::NoValue);

        let principal =
            crate::elaboration::elaborate_input("D(x)PrincipalValueIntegral(x/t,t,-1,1,{0})")
                .unwrap();
        let result = execute_elaborated(&mut engine, &principal, StepVerbosity::Detailed, false)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Completed);
        assert_eq!(result.value, "0");
    }

    #[test]
    fn multiple_integrals_compose_after_scoped_coordinate_evaluation() {
        let mut engine = RustEngine::spawn().unwrap();
        for (source, expected, operator) in [
            (
                "D(a)DoubleIntegral(x+y+a,y,0,1,x,0,1)",
                "1",
                CompositionOperator::DoubleIntegral,
            ),
            (
                "D(a)PolarIntegral(a,x,y,r,theta,0,1,0,2*Pi)",
                "Pi",
                CompositionOperator::PolarIntegral,
            ),
        ] {
            let input = crate::elaboration::elaborate_input(source).unwrap();
            let result = execute_elaborated(&mut engine, &input, StepVerbosity::Detailed, true)
                .unwrap()
                .unwrap();
            assert_eq!(result.status, CompositionStatus::Completed, "{source}");
            assert_eq!(result.value, expected, "{source}");
            assert!(result.operators.contains(&operator));
        }
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
        let error = execute_steps(&mut engine, "D(x,1,2,x)", StepVerbosity::Concise)
            .expect_err("malformed registered operations must be rejected");
        assert!(error.to_string().contains("4 个参数"));
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
        let migrated = "Factor(DoubleIntegral(f(x,y),y,0,x,x,0,1))";
        let result = execute_steps(&mut engine, migrated, StepVerbosity::Concise)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert_eq!(result.value, migrated);
        assert!(!result.steps.is_empty());

        let expression = "D(x)OdeSolveNumeric(y'==y,x,y,0,1,0.1)";
        let result = execute_steps(&mut engine, expression, StepVerbosity::Concise)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, CompositionStatus::Unresolved);
        assert!(result.value.starts_with("D(x)"));
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "numeric-ode-trajectory"));
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
