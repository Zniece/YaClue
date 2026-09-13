//! Release-level acceptance gate for the product-facing processing API.
//!
//! Keep this suite broad and contract-oriented. Detailed formula coverage
//! belongs in the domain capability tests; this is the single prerelease
//! entry point that proves every main domain has a successful path, an
//! honest non-success state, classified bad input, and structured evidence.

use processing::algebra::{transform, TransformKind};
use processing::arithmetic::execute_elaborated_structure;
use processing::elaboration::elaborate;
use processing::engine::{EngineError, RustEngine};
use processing::equations::{solve as solve_equations, SolveCompleteness, SolveStatus};
use processing::equivalence::{verify, Equivalence, ProofBudget, VerificationMethod};
use processing::improper_integrals::{evaluate as improper_integral, ImproperIntegralRequest};
use processing::intrinsics::{try_lower_improper_integral, IntrinsicKind};
use processing::limits::{limit, limit_steps, LimitDirection, LimitStatus};
use processing::linear_algebra::{
    compute as matrix_compute, linear_structure_steps, MatrixOperation,
};
use processing::numeric::{approximate, find_root, NumericKind, RootStatus};
use processing::objects::{DefinedObjectStatus, PrimitiveOperation};
use processing::ode::{solve as solve_ode, InitialCondition, OdeStatus};
use processing::ode_numeric::{solve_initial_value, NumericOdeOptions, NumericOdeStatus};
use processing::plot::{sample, SampleOptions, SampleTermination};
use processing::protocol::{Condition, ResolutionState};
use processing::semantic_core::{
    ComputationOutput, Effect, NormalizationLevel, OperatorId, SemanticInterpretation,
};
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
fn special_function_derivative_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");

    let closed = elaborate("D(x)IncompleteGamma(x^2,x+1)").unwrap();
    let input = &closed.children[1].object;
    let computation = execute_elaborated_structure(&mut engine, &closed).unwrap();
    let output = computation.value().expect("closed derivative value");
    assert_eq!(output.id, input.id);
    assert!(output.revision.0 > input.revision.0);
    assert_eq!(
        output.normalization.as_ref().unwrap().metadata.level,
        NormalizationLevel::Domain
    );
    assert!(output.print_source().contains("Integrate("));
    assert!(output
        .semantics
        .metadata
        .conditions
        .conditions()
        .iter()
        .any(|condition| matches!(condition, Condition::RealPartPositive { expression } if expression == "x+1")));
    assert!(computation
        .trace
        .as_ref()
        .unwrap()
        .events
        .iter()
        .any(|event| {
            event.rule == "derivative-registered-function-chain-rule"
                && event.input.object == input.id
                && event.output.object == output.id
        }));

    let before = input.operation_cache_key(
        format!("{:?}:x:1", OperatorId::Derivative),
        Vec::new(),
        None,
    );
    let after = output.operation_cache_key(
        format!("{:?}:x:1", OperatorId::Derivative),
        Vec::new(),
        None,
    );
    assert_ne!(
        before, after,
        "object revision must invalidate operation keys"
    );

    let formal = elaborate("D(x)HypergeometricPFQ({a,b},{c},Sin(x))").unwrap();
    let formal_input = &formal.children[1].object;
    let formal_result = execute_elaborated_structure(&mut engine, &formal).unwrap();
    assert!(matches!(formal_result.output, ComputationOutput::Held(_)));
    let held = formal_result.subject().unwrap();
    assert_eq!(held.id, formal_input.id);
    assert!(held.revision.0 > formal_input.revision.0);
    assert_eq!(
        held.semantics.metadata.resolution,
        ResolutionState::Unresolved
    );
    assert!(matches!(
        held.semantics.interpretation,
        SemanticInterpretation::HeldTypedApplication(ref application)
            if application.operator == OperatorId::Derivative
    ));
    assert!(formal_result
        .trace
        .as_ref()
        .unwrap()
        .events
        .iter()
        .any(|event| {
            event.rule == "derivative-known-formal-function"
                && event.input.object == formal_input.id
                && event.output.object == held.id
        }));
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

    let lowered = try_lower_improper_integral(
        &mut engine,
        &ImproperIntegralRequest {
            expression: "t^(1/x-1)*Exp(-t)".into(),
            variable: "t".into(),
            lower: "0".into(),
            upper: "Infinity".into(),
            singular_points: Vec::new(),
        },
        None,
    )
    .unwrap()
    .expect("Euler kernel should lower");
    assert_eq!(lowered.intrinsic, IntrinsicKind::Gamma);
    assert!(lowered.value.replace(' ', "").contains("Gamma((1/x))"));
    assert!(!lowered.conditions.is_empty());
}

#[test]
fn bounded_equivalence_release_contract() {
    let mut engine = RustEngine::spawn().unwrap();
    let equal = verify(
        &mut engine,
        "Integrate(t,0,X)Sin(t^2)",
        "Integrate(u,0,X)Sin(u^2)",
        ProofBudget::default(),
    )
    .unwrap();
    assert_eq!(equal.conclusion, Equivalence::ProvenEqual);
    assert_eq!(equal.method, Some(VerificationMethod::AlphaEquivalent));
    assert_eq!(equal.usage.cas_requests, 0);

    let different = verify(&mut engine, "Sin(x)", "Cos(x)", ProofBudget::default()).unwrap();
    assert_eq!(different.conclusion, Equivalence::ProvenDifferent);
    assert!(matches!(
        different.method,
        Some(VerificationMethod::NumericCounterexample) | Some(VerificationMethod::ResidualProof)
    ));
}

#[test]
fn compact_representation_release_contract() {
    let mut engine = RustEngine::spawn().unwrap();
    let expression = elaborate("Expand(D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t))))").unwrap();
    let computation = execute_elaborated_structure(&mut engine, &expression).unwrap();
    let output = computation.value().unwrap();
    let source = output.print_source().replace(' ', "");

    assert!(source.contains("Gamma(x)"));
    assert!(source.contains("PolyGamma(0,x)"));
    assert!(!source.contains("Integrate("));
    assert!(output.stable_representation_count() <= 4);
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

#[test]
fn migrated_object_pipeline_release_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let execute = |engine: &mut RustEngine, source: &str| {
        let elaborated = elaborate(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        execute_elaborated_structure(engine, &elaborated)
            .unwrap_or_else(|error| panic!("{source}: {error}"))
    };

    for (source, expected) in [
        ("Sum(k,1,10,k)", "55"),
        ("ImproperIntegral(1/(1+x^2),x,0,Infinity)", "Pi/2"),
        ("PrincipalValueIntegral(1/x,x,-1,1,{0})", "0"),
        ("DoubleIntegral(x+y,y,0,2,x,0,1)", "3"),
    ] {
        let result = execute(&mut engine, source);
        assert!(
            matches!(result.output, ComputationOutput::Value(_)),
            "{source}"
        );
        assert_eq!(result.value().unwrap().print_source(), expected, "{source}");
    }

    let polar = execute(&mut engine, "PolarIntegral(x^2+y^2,x,y,r,theta,0,1,0,2*Pi)");
    assert!(matches!(polar.output, ComputationOutput::Value(_)));
    assert!(polar.value().unwrap().print_source().contains("Pi"));

    let root = execute(&mut engine, "FindRoot(x^2-2,x,1)");
    assert!(matches!(root.output, ComputationOutput::Value(_)));
    assert_eq!(
        root.value().unwrap().semantics.metadata.exactness,
        processing::semantic::Exactness::Approximate
    );

    let numeric_ode = execute(&mut engine, "OdeSolveNumeric(y'==y,x,y,0,1,0.1)");
    assert!(matches!(numeric_ode.output, ComputationOutput::Value(_)));
    assert_eq!(
        numeric_ode.value().unwrap().semantics.kind,
        processing::semantic::ValueKind::SampledData
    );

    let plot = execute(&mut engine, "Plot(D(x)(x^2),x,0,1)");
    assert!(matches!(plot.output, ComputationOutput::EffectsOnly));
    assert!(plot.subject().is_none());
    assert!(
        matches!(plot.effects.as_slice(), [Effect::Plot(effect)] if effect.expression == "2*x")
    );

    let extrema = execute(&mut engine, "Extrema(Expand((x-1)^2+(y+2)^2),x,y)");
    assert!(matches!(extrema.output, ComputationOutput::Value(_)));
    assert_eq!(
        extrema.value().unwrap().semantics.kind,
        processing::semantic::ValueKind::SolutionSet
    );
    assert!(extrema
        .certificates
        .iter()
        .any(|item| item.kind == "extrema_analysis"));

    let lagrange = execute(&mut engine, "Lagrange(x+y,x^2+y^2-1,x,y)");
    assert!(matches!(lagrange.output, ComputationOutput::Value(_)));
    assert!(lagrange
        .certificates
        .iter()
        .any(|item| item.kind == "lagrange_analysis"));

    let absent = execute(&mut engine, "Extrema(x+y,x,y)");
    assert!(matches!(absent.output, ComputationOutput::NoValue(_)));
    let held = execute(&mut engine, "Lagrange(x+y,(x^2+y^2)^2,x,y)");
    assert!(matches!(held.output, ComputationOutput::Held(_)));
}
