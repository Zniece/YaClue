use processing::composition::execute_steps;
use processing::engine::RustEngine;
use processing::steps::{StepKind, StepVerbosity};

#[test]
fn difficult_compositions_are_continuous_whole_expression_chains() {
    let mut engine = RustEngine::spawn().unwrap();
    for source in [
        "D(x)Limit(t,0)(Sin(t)/t+x^2)",
        "D(x)Integrate(t)(Limit(u,0)(Sin(u)/u+t*x))",
        "Sin(Limit(t,0)(Sin(t)/t))",
        "{Limit(t,0)(Sin(t)/t),D(x)(x^3),Integrate(x)(2*x)}",
        "(y+2^2*y)==Sin(x)",
    ] {
        let result = execute_steps(&mut engine, source, StepVerbosity::Detailed)
            .unwrap_or_else(|error| panic!("{source}: {error}"))
            .unwrap();
        assert!(!result.steps.is_empty(), "{source}: {result:#?}");
        assert!(result.steps.iter().all(|step| {
            step.kind == StepKind::EquivalentTransformation
                && step
                    .before_expr
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                && step
                    .before_tex
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                && !step.expr.is_empty()
                && !step.tex.is_empty()
        }));
        assert!(
            result
                .steps
                .windows(2)
                .all(|pair| { pair[1].before_expr.as_deref() == Some(pair[0].expr.as_str()) }),
            "{source}: {:#?}",
            result.steps
        );
        assert_eq!(result.steps.last().unwrap().expr, result.value, "{source}");
    }
}

#[test]
fn product_steps_never_expose_internal_execution_vocabulary() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = execute_steps(
        &mut engine,
        "D(x)ImproperIntegral(t^(x-1)*Exp(-t),t,0,Infinity)",
        StepVerbosity::Detailed,
    )
    .unwrap()
    .unwrap();
    for step in result.steps {
        let text = format!("{} {}", step.rule, step.why).to_ascii_lowercase();
        for forbidden in ["typed", "ast", "container", "rebuild", "lowering"] {
            assert!(!text.contains(forbidden), "{step:#?}");
        }
        assert!(!text.contains("类型化"), "{step:#?}");
        assert!(!text.contains("重建"), "{step:#?}");
    }
}
