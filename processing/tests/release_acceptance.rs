//! Release-level acceptance gate for the product-facing processing API.
//!
//! Keep this suite broad and contract-oriented. Detailed formula coverage
//! belongs in the domain capability tests; this is the single prerelease
//! entry point that proves every main domain has a successful path, an
//! honest non-success state, classified bad input, and structured evidence.

use processing::algebra::{transform, TransformKind};
use processing::engine::{EngineError, RustEngine};
use processing::equations::{solve as solve_equations, SolveCompleteness, SolveStatus};
use processing::improper_integrals::{evaluate as improper_integral, ImproperIntegralRequest};
use processing::limits::{limit, limit_steps, LimitDirection, LimitStatus};
use processing::linear_algebra::{
    compute as matrix_compute, linear_structure_steps, MatrixOperation,
};
use processing::numeric::{approximate, find_root, NumericKind, RootStatus};
use processing::objects::{DefinedObjectStatus, PrimitiveOperation};
use processing::ode::{solve as solve_ode, InitialCondition, OdeStatus};
use processing::ode_numeric::{solve_initial_value, NumericOdeOptions, NumericOdeStatus};
use processing::plot::{sample, SampleOptions, SampleTermination};
use processing::steps::{derive_integrals, derive_steps};

fn assert_invalid_input(result: Result<impl Sized, EngineError>) {
    assert!(
        matches!(result, Err(EngineError::InvalidInput(_))),
        "expected classified invalid input"
    );
}

fn assert_step_contract(steps: &[processing::steps::Step]) {
    assert!(!steps.is_empty());
    assert!(steps.iter().all(|step| {
        !step.rule.is_empty()
            && !step.expr.is_empty()
            && !step.why.is_empty()
            && !step.tex.is_empty()
    }));
}

#[test]
fn algebra_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let expanded = transform(&mut engine, "(x+1)^2", TransformKind::Expand, None).unwrap();
    assert!(expanded.changed && !expanded.unresolved && !expanded.tex.is_empty());

    let unsupported = transform(&mut engine, "Sin(x)", TransformKind::Factor, None).unwrap();
    assert!(
        unsupported.unresolved,
        "unsupported transforms must remain explicit"
    );

    assert_invalid_input(transform(&mut engine, "x^2", TransformKind::Apart, None));
}

#[test]
fn calculus_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let derivative = derive_steps(&mut engine, "Sin(x)^2", "x").unwrap();
    assert_step_contract(&derivative);
    assert!(
        derivative.last().unwrap().expr.contains("Sin")
            || derivative.last().unwrap().expr.contains("Cos")
    );

    let integral = derive_integrals(&mut engine, "x*Exp(x)", "x").unwrap();
    assert_step_contract(&integral);

    let unsupported = derive_integrals(&mut engine, "Sin(x^x)", "x").unwrap();
    assert_step_contract(&unsupported);
    assert!(
        unsupported.last().unwrap().expr.starts_with("Integrate("),
        "unsupported integrals must remain visibly unevaluated"
    );
    assert_invalid_input(derive_steps(&mut engine, "x^2", "x;Echo(1)"));
}

#[test]
fn defined_object_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let request = ImproperIntegralRequest {
        expression: "Exp(-x)".into(),
        variable: "x".into(),
        lower: "0".into(),
        upper: "Infinity".into(),
        singular_points: Vec::new(),
    };
    let converged = improper_integral(&mut engine, &request, None).unwrap();
    assert_eq!(converged.status, DefinedObjectStatus::Converged);
    assert_eq!(converged.value, "1");
    assert!(converged
        .components
        .iter()
        .any(|component| component.operation == PrimitiveOperation::OneSidedLimit));

    let divergent = improper_integral(
        &mut engine,
        &ImproperIntegralRequest {
            expression: "1/x".into(),
            variable: "x".into(),
            lower: "-1".into(),
            upper: "1".into(),
            singular_points: vec!["0".into()],
        },
        None,
    )
    .unwrap();
    assert_eq!(divergent.status, DefinedObjectStatus::Divergent);
    assert!(!divergent
        .components
        .iter()
        .any(|component| component.operation == PrimitiveOperation::Assemble));

    assert_invalid_input(improper_integral(
        &mut engine,
        &ImproperIntegralRequest {
            singular_points: vec!["outside".into()],
            ..request
        },
        None,
    ));
}

#[test]
fn limits_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let result = limit(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
    assert_eq!(result.status, LimitStatus::Converged);
    assert_eq!(result.value, "1");
    assert!(!result.tex.is_empty());

    let divergent = limit(&mut engine, "1/x", "x", "0", LimitDirection::Both).unwrap();
    assert_eq!(divergent.status, LimitStatus::DoesNotExist);

    let steps = limit_steps(&mut engine, "Sin(x)/x", "x", "0", LimitDirection::Both).unwrap();
    assert_step_contract(&steps);
    assert_invalid_input(limit(
        &mut engine,
        "x",
        "x;Echo(1)",
        "0",
        LimitDirection::Both,
    ));
}

#[test]
fn equations_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let solved = solve_equations(&mut engine, &["x^2-1==0"], &["x"]).unwrap();
    assert_eq!(solved.status, SolveStatus::Solved);
    assert_eq!(solved.solutions.len(), 2);
    assert_eq!(solved.completeness, SolveCompleteness::Complete);
    assert!(!solved.tex.is_empty());

    let unresolved = solve_equations(&mut engine, &["x^5-x+1==0"], &["x"]).unwrap();
    assert_eq!(unresolved.status, SolveStatus::Unresolved);
    assert_eq!(unresolved.completeness, SolveCompleteness::Unknown);

    assert_invalid_input(solve_equations(&mut engine, &["x==1"], &["x;Echo(1)"]));
}

#[test]
fn ode_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let solved = solve_ode(&mut engine, "y'+y==0", "x", "y", &[]).unwrap();
    assert_eq!(solved.status, OdeStatus::Solved);
    assert_eq!(solved.residual, "0");
    assert!(!solved.solution_branches.is_empty());

    let unresolved = solve_ode(&mut engine, "y''+Sin(y)==0", "x", "y", &[]).unwrap();
    assert_eq!(unresolved.status, OdeStatus::Unresolved);
    assert_ne!(unresolved.residual, "0");

    assert_invalid_input(solve_ode(&mut engine, "y'''==0", "x", "y", &[]));
}

#[test]
fn linear_algebra_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let reduced = linear_structure_steps(&mut engine, "{{1,2},{2,4}}").unwrap();
    assert_eq!(reduced.result.rank, 1);
    assert_eq!(reduced.result.nullity, 1);
    assert!(!reduced.operations.is_empty());
    assert_step_contract(&reduced.steps);

    let unresolved = matrix_compute(
        &mut engine,
        "{{a,b},{c,d}}",
        MatrixOperation::Eigenvalues,
        None,
    )
    .unwrap();
    assert!(unresolved.unresolved, "unexpected result: {unresolved:?}");

    assert_invalid_input(matrix_compute(
        &mut engine,
        "{{1,0},{0,1}}",
        MatrixOperation::Add,
        None,
    ));
}

#[test]
fn numeric_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let approximation = approximate(&mut engine, "Pi", 40).unwrap();
    assert_eq!(approximation.kind, NumericKind::ApproximateReal);
    assert_eq!(approximation.precision_digits, 40);
    assert!(!approximation.tex.is_empty());

    let no_root = find_root(&mut engine, "x^2+1", "x", 1.0, 1e-10, Some((0.0, 2.0))).unwrap();
    assert_eq!(no_root.status, RootStatus::NoConvergence);

    assert_invalid_input(approximate(&mut engine, "Pi", 0));

    let limited = solve_initial_value(
        &mut engine,
        "y'==y",
        "x",
        "y",
        &[InitialCondition {
            derivative_order: 0,
            point: "0",
            value: "1",
        }],
        NumericOdeOptions {
            max_evaluations: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(limited.status, NumericOdeStatus::EvaluationLimit);
}

#[test]
fn plotting_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let plot = sample(
        &mut engine,
        "Sin(x)",
        "x",
        (0.0, std::f64::consts::PI),
        &SampleOptions::default(),
    )
    .unwrap();
    assert_eq!(plot.termination, SampleTermination::Complete);
    assert!(!plot.points.is_empty() && !plot.segments.is_empty());
    assert!(plot.suggested_bounds.is_some());

    let limited = sample(
        &mut engine,
        "x^2",
        "x",
        (0.0, 10.0),
        &SampleOptions {
            points: 4,
            max_depth: 8,
            eps: 1e-12,
            batch: 16,
            max_points: 9,
        },
    )
    .unwrap();
    assert_eq!(limited.termination, SampleTermination::PointLimit);

    assert_invalid_input(sample(
        &mut engine,
        "x",
        "x",
        (1.0, 0.0),
        &SampleOptions::default(),
    ));
}
