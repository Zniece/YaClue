use super::*;

#[test]
fn unified_input_routes_defined_integrals_as_semantic_objects() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let converged = process_expression_with_engine(
        request("ImproperIntegral(1/(1+x^2),x,0,Infinity)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(converged.kind, "defined_object");
    assert!(converged.expression.contains("Pi"));
    assert_eq!(converged.semantic.kind, ValueKind::Scalar);
    assert!(!converged.steps.is_empty());
    assert!(converged
        .steps
        .iter()
        .any(|step| step.rule.starts_with("object-")));

    let divergent = process_expression_with_engine(
        request("ImproperIntegral(1/x,x,-1,1,{0})", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(
        divergent.outcome.reason,
        Some(processing::protocol::OutcomeReason::Divergent)
    );

    let principal = process_expression_with_engine(
        request("PrincipalValueIntegral(1/x,x,-1,1,{0})", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(principal.expression, "0");
    assert_eq!(
        principal.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
}

#[test]
fn unified_input_routes_multiple_integrals_as_semantic_objects() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let double = process_expression_with_engine(
        request("DoubleIntegral(x+y,y,0,2,x,0,1)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(double.kind, "double_integral");
    assert_eq!(double.expression, "3");
    assert!(double.semantic.symbols.is_empty());
    assert!(double.semantic.bound_symbols.is_empty());
    assert!(double
        .steps
        .iter()
        .any(|step| step.rule == "iterated-integral-inner"));

    let polar = process_expression_with_engine(
        request("PolarIntegral(x^2+y^2,x,y,r,theta,0,1,0,2*Pi)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(polar.kind, "polar_integral");
    assert!(polar.expression.contains("Pi"));
    assert!(polar.steps.iter().any(|step| step.rule == "polar-jacobian"));

    let composed = process_expression_with_engine(
        request("D(a)DoubleIntegral(x+y+a,y,0,1,x,0,1)", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(composed.expression, "1");
    assert_eq!(composed.kind, "composition");
}

#[test]
fn unified_input_exposes_numeric_ode_as_terminal_sampled_data() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let result = process_expression_with_engine(
        request("OdeSolveNumeric(y'==y,x,y,0,1,0.1)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(result.kind, "numeric_ode");
    assert_eq!(result.semantic.kind, ValueKind::SampledData);
    assert_eq!(
        result.semantic.exactness,
        processing::semantic::Exactness::Approximate
    );
    assert!(result.expression.starts_with("{{0,"));
    let sampled = result.sampled_data.as_ref().unwrap();
    assert_eq!(sampled.independent, "x");
    assert_eq!(sampled.dependent, "y");
    assert!(sampled.points.len() > 1);
    assert!(result
        .steps
        .iter()
        .any(|step| step.rule == "numeric-ode-trajectory"));

    let composed = process_expression_with_engine(
        request("D(x)OdeSolveNumeric(y'==y,x,y,0,1,0.1)", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(
        composed.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(composed.expression.starts_with("D(x)"));
}

#[test]
fn unified_input_exposes_find_root_as_a_composable_approximate_scalar() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let root =
        process_expression_with_engine(request("FindRoot(x^2-2,x,1)", true), &mut engine).unwrap();
    assert_eq!(root.kind, "numeric_root");
    assert_eq!(root.semantic.kind, ValueKind::Scalar);
    assert_eq!(
        root.outcome.exactness,
        processing::semantic::Exactness::Approximate
    );
    assert_eq!(
        root.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
    assert!(root.steps.iter().any(|step| step.rule == "numeric-root"));

    let composed =
        process_expression_with_engine(request("N(FindRoot(x^2-2,x,1),12)", false), &mut engine)
            .unwrap();
    assert_eq!(composed.kind, "composition");
    assert_eq!(composed.semantic.kind, ValueKind::Scalar);

    let differentiated =
        process_expression_with_engine(request("D(x)FindRoot(x^2-2,x,1)", false), &mut engine)
            .unwrap();
    assert_eq!(differentiated.expression, "0");
}

#[test]
fn unified_input_exposes_plot_as_a_terminal_structured_effect() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let result =
        process_expression_with_engine(request("Plot(D(x)(x^2),x,0,3.14)", true), &mut engine)
            .unwrap();
    assert_eq!(result.kind, "plot");
    assert_eq!(result.semantic.kind, ValueKind::Expression);
    assert_eq!(result.expression, "2*x");
    assert!(result.effect_only);
    let plot = result.plot.as_ref().unwrap();
    assert_eq!(plot.expression, "2*x");
    assert_eq!(plot.variable, "x");
    assert!(!plot.sampled.points.is_empty());
    assert!(result.steps.iter().any(|step| step.rule == "power-rule"));

    for expression in ["Plot(x,x,0,1)+1", "Sin(Plot(x,x,0,1))"] {
        let error = match process_expression_with_engine(request(expression, false), &mut engine) {
            Ok(_) => panic!("plot effects cannot enter mathematical composition"),
            Err(error) => error,
        };
        assert!(error.message.contains("副作用"), "{}", error.message);
    }
}

#[test]
fn unified_input_routes_extrema_and_lagrange_as_structured_candidate_sets() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let extrema = process_expression_with_engine(
        request("Extrema(Expand((x-1)^2+(y+2)^2),x,y)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(extrema.kind, "extrema");
    assert_eq!(extrema.semantic.kind, ValueKind::SolutionSet);
    let Some(processing::semantic_core::ComputationAnalysis::Extrema(extrema_analysis)) =
        extrema.analysis.as_ref()
    else {
        panic!("expected typed extrema analysis")
    };
    assert_eq!(
        extrema_analysis.status,
        processing::extrema::ExtremaStatus::Classified
    );
    assert_eq!(
        extrema_analysis.critical_points[0].kind,
        processing::extrema::CriticalPointKind::LocalMinimum
    );
    assert!(extrema_analysis.critical_points[0].gradient_verified);
    assert!(extrema.steps.iter().all(|step| {
        step.before_expr
            .as_deref()
            .is_some_and(|before| !before.is_empty())
            && !step.expr.is_empty()
    }));

    let lagrange =
        process_expression_with_engine(request("Lagrange(x+y,x^2+y^2-1,x,y)", true), &mut engine)
            .unwrap();
    assert_eq!(lagrange.kind, "lagrange");
    assert_eq!(lagrange.semantic.kind, ValueKind::SolutionSet);
    let Some(processing::semantic_core::ComputationAnalysis::Lagrange(lagrange_analysis)) =
        lagrange.analysis.as_ref()
    else {
        panic!("expected typed Lagrange analysis")
    };
    assert_eq!(
        lagrange_analysis.status,
        processing::extrema::LagrangeStatus::Candidates
    );
    assert!(!lagrange_analysis.candidates.is_empty());
    assert!(lagrange_analysis
        .candidates
        .iter()
        .all(|candidate| candidate.stationarity_verified && candidate.constraint_verified));

    let absent =
        process_expression_with_engine(request("Extrema(x+y,x,y)", false), &mut engine).unwrap();
    assert_eq!(
        absent.outcome.resolution,
        processing::protocol::ResolutionState::NoResult
    );
}

#[test]
fn unified_input_routes_typed_matrix_decompositions() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for (expression, output_head, rule) in [
        (
            "PLDU({{4,2},{2,2}})",
            "PLDUDecomposition(",
            "matrix-pldu-decomposition",
        ),
        (
            "Cholesky({{4,2},{2,2}})",
            "CholeskyDecomposition(",
            "matrix-cholesky-decomposition",
        ),
        (
            "GramSchmidt({{1,0},{1,1}})",
            "OrthonormalBasisObject(",
            "matrix-orthonormal-basis",
        ),
    ] {
        let result =
            process_expression_with_engine(request(expression, true), &mut engine).unwrap();
        assert_eq!(result.kind, "matrix", "{expression}");
        assert!(result.expression.starts_with(output_head), "{expression}");
        assert!(
            result.steps.iter().any(|step| step.rule == rule),
            "{expression}"
        );
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Solved,
            "{expression}"
        );
    }

    let factors =
        process_expression_with_engine(request("Factors(PLDU({{4,2},{2,2}}))", true), &mut engine)
            .unwrap();
    assert!(factors.expression.starts_with("{{"));
    assert!(factors
        .steps
        .iter()
        .any(|step| step.rule == "matrix-pldu-decomposition"));
    assert!(factors
        .steps
        .iter()
        .any(|step| step.rule == "matrix-factor-projection"));
    assert_eq!(
        factors.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
}
