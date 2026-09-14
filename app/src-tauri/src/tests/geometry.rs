use super::*;

#[test]
fn unified_input_exposes_composable_multivariate_differentials() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let gradient = process_expression_with_engine(
        request("Gradient(x^2+y^2,{x,y},{1,-2})", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(gradient.kind, "multivariate");
    assert_eq!(gradient.expression.replace(' ', ""), "{2,-4}");
    assert!(matches!(
        gradient.analysis.as_ref(),
        Some(processing::semantic_core::ComputationAnalysis::MultivariateShape(shape))
            if shape == &[2]
    ));
    assert!(gradient
        .steps
        .iter()
        .any(|step| step.rule == "multivariate-differential"));
    assert_eq!(
        gradient.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );

    let composed =
        process_expression_with_engine(request("Sin(Divergence({x,y},{x,y}))", false), &mut engine)
            .unwrap();
    assert_eq!(composed.kind, "composition");
    assert_eq!(composed.expression, "Sin(2)");
    assert_eq!(composed.semantic.kind, ValueKind::Scalar);
}

#[test]
fn unified_input_exposes_composable_line_integrals() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let line = process_expression_with_engine(
        request("VectorLineIntegral({y,x},{x,y},{t,t^2},t,0,1)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(line.kind, "line_integral");
    assert_eq!(line.expression, "1");
    assert!(matches!(
        line.analysis.as_ref(),
        Some(processing::semantic_core::ComputationAnalysis::LineIntegral(result))
            if result.integrand_verified
    ));
    assert!(line
        .steps
        .iter()
        .any(|step| step.rule == "line-integral-pullback"));
    assert_eq!(
        line.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );

    let composed = process_expression_with_engine(
        request("D(a)(a*ScalarLineIntegral(x,{x,y},{t,0},t,0,1))", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(composed.kind, "composition");
    assert_eq!(composed.expression, "1/2");
}

#[test]
fn unified_input_exposes_composable_surface_integrals() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let surface = process_expression_with_engine(
        request(
            "VectorSurfaceIntegral({0,0,1},{x,y,z},{u,v,0},{u,v},{0,0},{2,3},Reversed)",
            true,
        ),
        &mut engine,
    )
    .unwrap();
    assert_eq!(surface.kind, "surface_integral");
    assert_eq!(surface.expression, "-6");
    assert!(matches!(
        surface.analysis.as_ref(),
        Some(processing::semantic_core::ComputationAnalysis::SurfaceIntegral(result))
            if result.normal_verified && result.integrand_verified
    ));
    assert!(surface
        .steps
        .iter()
        .any(|step| step.rule == "surface-integral-normal"));
    assert_eq!(
        surface.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );

    let composed = process_expression_with_engine(
        request(
            "D(a)(a*ScalarSurfaceIntegral(1,{x,y,z},{u,v,0},{u,v},{0,0},{2,3}))",
            false,
        ),
        &mut engine,
    )
    .unwrap();
    assert_eq!(composed.kind, "composition");
    assert_eq!(composed.expression, "6");
}
