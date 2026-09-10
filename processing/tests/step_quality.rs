use processing::engine::RustEngine;
use processing::equations;
use processing::extrema;
use processing::limits::{self, LimitDirection};
use processing::line_integrals::{self, LineIntegralKind, LineIntegralRequest};
use processing::linear_algebra;
use processing::multiple_integrals::{self, IntegralBound, PolarRegion};
use processing::ode;
use processing::steps::{Step, StepImportance};
use processing::surface_integrals::{
    self, SurfaceIntegralKind, SurfaceIntegralRequest, SurfaceOrientation,
};

fn assert_teaching_steps(domain: &str, steps: &[Step]) {
    assert!(steps.len() >= 2, "{domain} has no explanatory chain");
    for (index, step) in steps.iter().enumerate() {
        assert!(
            !step.rule.trim().is_empty(),
            "{domain} step {index} has no rule"
        );
        assert!(
            !step.expr.trim().is_empty(),
            "{domain} step {index} has no expression"
        );
        assert!(
            !step.why.trim().is_empty(),
            "{domain} step {index} has no explanation"
        );
        assert!(
            !step.tex.trim().is_empty(),
            "{domain} step {index} has no TeX"
        );
        if let Some(previous) = index.checked_sub(1).map(|previous| &steps[previous]) {
            assert!(
                previous.rule != step.rule || previous.expr != step.expr,
                "{domain} repeats an identical adjacent step: {}",
                step.rule
            );
        }
    }
    assert_eq!(
        steps.last().unwrap().importance,
        StepImportance::Key,
        "{domain} final result is not marked key"
    );
}

#[test]
fn equation_limit_and_extrema_steps_have_complete_teaching_fields() {
    let mut engine = RustEngine::spawn().unwrap();
    assert_teaching_steps(
        "equation",
        &equations::solve_steps(&mut engine, "x^2-3*x+2==0", "x")
            .unwrap()
            .steps,
    );
    assert_teaching_steps(
        "limit",
        &limits::limit_steps(
            &mut engine,
            "(1-Cos(x))/x^2",
            "x",
            "0",
            LimitDirection::Both,
        )
        .unwrap(),
    );
    assert_teaching_steps(
        "ode",
        &ode::solve_steps(&mut engine, "y'+y==x", "x", "y", &[])
            .unwrap()
            .steps,
    );
    assert_teaching_steps(
        "extrema",
        &extrema::analyze_steps(&mut engine, "x^2+y^2", "x", "y")
            .unwrap()
            .steps,
    );
    assert_teaching_steps(
        "lagrange",
        &extrema::analyze_lagrange_steps(&mut engine, "x+y", "x^2+y^2-1", "x", "y")
            .unwrap()
            .steps,
    );
}

#[test]
fn multivariable_integral_steps_show_complete_formula_chains() {
    let mut engine = RustEngine::spawn().unwrap();
    let inner = IntegralBound {
        variable: "y",
        lower: "0",
        upper: "x",
    };
    let outer = IntegralBound {
        variable: "x",
        lower: "0",
        upper: "1",
    };
    assert_teaching_steps(
        "double integral",
        &multiple_integrals::double_integral_steps(&mut engine, "x+y", inner, outer)
            .unwrap()
            .steps,
    );
    let inner = IntegralBound {
        variable: "z",
        lower: "0",
        upper: "x+y",
    };
    let middle = IntegralBound {
        variable: "y",
        lower: "0",
        upper: "x",
    };
    let outer = IntegralBound {
        variable: "x",
        lower: "0",
        upper: "1",
    };
    assert_teaching_steps(
        "triple integral",
        &multiple_integrals::triple_integral_steps(&mut engine, "1", inner, middle, outer)
            .unwrap()
            .steps,
    );
    assert_teaching_steps(
        "polar integral",
        &multiple_integrals::polar_integral_steps(
            &mut engine,
            "1",
            "x",
            "y",
            "r",
            "theta",
            PolarRegion {
                radial_lower: "0",
                radial_upper: "1",
                angle_lower: "0",
                angle_upper: "Pi/2",
            },
        )
        .unwrap()
        .steps,
    );

    let line = LineIntegralRequest {
        kind: LineIntegralKind::VectorWork,
        field: vec!["y".into(), "x".into()],
        coordinates: vec!["x".into(), "y".into()],
        curve: vec!["t".into(), "t^2".into()],
        parameter: "t".into(),
        lower: "0".into(),
        upper: "1".into(),
    };
    let line_steps = line_integrals::compute_steps(&mut engine, &line)
        .unwrap()
        .steps;
    assert_teaching_steps("line integral", &line_steps);
    assert!(line_steps
        .iter()
        .any(|step| step.expr.starts_with("Integrate(t,0,1)")));

    let surface = SurfaceIntegralRequest {
        kind: SurfaceIntegralKind::VectorFlux,
        orientation: SurfaceOrientation::ParameterOrder,
        field: vec!["0".into(), "0".into(), "1".into()],
        coordinates: ["x".into(), "y".into(), "z".into()],
        surface: ["u".into(), "v".into(), "u+v".into()],
        parameters: ["u".into(), "v".into()],
        lower: ["0".into(), "0".into()],
        upper: ["1".into(), "1".into()],
    };
    let surface_steps = surface_integrals::compute_steps(&mut engine, &surface)
        .unwrap()
        .steps;
    assert_teaching_steps("surface integral", &surface_steps);
    assert!(surface_steps
        .iter()
        .any(|step| step.expr.starts_with("Integrate(v,0,1)Integrate(u,0,1)")));
}

#[test]
fn row_reduction_steps_have_complete_teaching_fields() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = linear_algebra::linear_structure_steps(&mut engine, "{{0,2},{1,1}}").unwrap();
    assert_teaching_steps("row reduction", &result.steps);
    assert!(result.steps.iter().any(|step| step.rule == "row-swap"));
    assert!(result.steps.iter().any(|step| step.rule == "row-scale"));
    assert!(result.steps.iter().any(|step| step.rule == "row-eliminate"));
}
