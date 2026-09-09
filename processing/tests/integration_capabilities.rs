use processing::engine::{Engine, RustEngine};
use processing::line_integrals::{
    compute as line_integral, compute_steps as line_integral_steps, LineIntegralKind,
    LineIntegralRequest,
};
use processing::multiple_integrals::{
    double_integral, double_integral_steps, polar_integral, polar_integral_steps, triple_integral,
    triple_integral_steps, IntegralBound, IteratedIntegralStatus, PolarRegion,
    TripleIntegralStatus,
};
use processing::surface_integrals::{
    compute as surface_integral, compute_steps as surface_integral_steps, SurfaceIntegralKind,
    SurfaceIntegralRequest, SurfaceOrientation,
};

fn assert_equivalent(engine: &mut dyn Engine, actual: &str, expected: &str) {
    let check = engine
        .eval(&format!("IsZero(Simplify(({actual})-({expected})))"))
        .unwrap();
    assert_eq!(check.expr.to_string(), "True", "{actual} != {expected}");
}

fn line_request(kind: LineIntegralKind, field: &[&str]) -> LineIntegralRequest {
    LineIntegralRequest {
        kind,
        field: field.iter().map(|value| (*value).into()).collect(),
        coordinates: vec!["x".into(), "y".into()],
        curve: vec!["t".into(), "t^2".into()],
        parameter: "t".into(),
        lower: "0".into(),
        upper: "1".into(),
    }
}

fn surface_request(kind: SurfaceIntegralKind, field: &[&str]) -> SurfaceIntegralRequest {
    SurfaceIntegralRequest {
        kind,
        orientation: SurfaceOrientation::ParameterOrder,
        field: field.iter().map(|value| (*value).into()).collect(),
        coordinates: ["x".into(), "y".into(), "z".into()],
        surface: ["u".into(), "v".into(), "u+v".into()],
        parameters: ["u".into(), "v".into()],
        lower: ["0".into(), "0".into()],
        upper: ["1".into(), "1".into()],
    }
}

#[test]
fn iterated_integrals_cover_fixed_and_variable_bounds_with_steps() {
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
    let double = double_integral(&mut engine, "x+y", inner, outer).unwrap();
    assert_eq!(double.status, IteratedIntegralStatus::Evaluated);
    assert_equivalent(&mut engine, &double.value, "1/2");
    let double_steps = double_integral_steps(&mut engine, "x+y", inner, outer).unwrap();
    assert_eq!(
        double_steps.steps.last().unwrap().expr,
        double_steps.result.value
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
    let triple = triple_integral(&mut engine, "1", inner, middle, outer).unwrap();
    assert_eq!(triple.status, TripleIntegralStatus::Evaluated);
    assert_eq!(triple.layers.len(), 3);
    assert_equivalent(&mut engine, &triple.value, "1/2");
    let triple_steps = triple_integral_steps(&mut engine, "1", inner, middle, outer).unwrap();
    assert_eq!(
        triple_steps.steps.last().unwrap().expr,
        triple_steps.result.value
    );
}

#[test]
fn coordinate_and_parametric_integrals_return_verified_constructions() {
    let mut engine = RustEngine::spawn().unwrap();
    let region = PolarRegion {
        radial_lower: "0",
        radial_upper: "2",
        angle_lower: "0",
        angle_upper: "Pi/2",
    };
    let polar = polar_integral(&mut engine, "x^2+y^2", "x", "y", "r", "theta", region).unwrap();
    assert!(polar.jacobian_verified);
    assert_eq!(polar.integral.status, IteratedIntegralStatus::Evaluated);
    assert_equivalent(&mut engine, &polar.integral.value, "2*Pi");
    let polar_steps =
        polar_integral_steps(&mut engine, "x^2+y^2", "x", "y", "r", "theta", region).unwrap();
    assert_eq!(
        polar_steps.steps.last().unwrap().expr,
        polar_steps.result.integral.value
    );

    let line_request = line_request(LineIntegralKind::VectorWork, &["y", "x"]);
    let line = line_integral(&mut engine, &line_request).unwrap();
    assert!(line.completed && line.integrand_verified);
    assert_equivalent(&mut engine, &line.value, "1");
    let line_steps = line_integral_steps(&mut engine, &line_request).unwrap();
    assert_eq!(
        line_steps.steps.last().unwrap().expr,
        line_steps.result.value
    );

    let surface_request = surface_request(SurfaceIntegralKind::VectorFlux, &["0", "0", "1"]);
    let surface = surface_integral(&mut engine, &surface_request).unwrap();
    assert!(surface.completed && surface.normal_verified && surface.integrand_verified);
    assert_equivalent(&mut engine, &surface.value, "1");
    let surface_steps = surface_integral_steps(&mut engine, &surface_request).unwrap();
    assert_eq!(
        surface_steps.steps.last().unwrap().expr,
        surface_steps.result.value
    );
}

#[test]
fn unresolved_integrals_stop_at_the_first_unavailable_layer() {
    let mut engine = RustEngine::spawn().unwrap();
    let double = double_integral(
        &mut engine,
        "Sin(y^y)",
        IntegralBound {
            variable: "y",
            lower: "0",
            upper: "1",
        },
        IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        },
    )
    .unwrap();
    assert_eq!(double.status, IteratedIntegralStatus::InnerUnresolved);
    assert!(double.outer.is_none());

    let triple = triple_integral(
        &mut engine,
        "Sin(y^y)",
        IntegralBound {
            variable: "z",
            lower: "0",
            upper: "1",
        },
        IntegralBound {
            variable: "y",
            lower: "0",
            upper: "1",
        },
        IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        },
    )
    .unwrap();
    assert_eq!(triple.status, TripleIntegralStatus::MiddleUnresolved);
    assert_eq!(triple.layers.len(), 2);
}

#[test]
fn invalid_regions_and_parameter_dependencies_are_rejected() {
    let mut engine = RustEngine::spawn().unwrap();
    assert!(polar_integral(
        &mut engine,
        "1",
        "x",
        "y",
        "r",
        "theta",
        PolarRegion {
            radial_lower: "-1",
            radial_upper: "2",
            angle_lower: "0",
            angle_upper: "2*Pi",
        },
    )
    .is_err());

    let invalid_line = line_request(LineIntegralKind::ScalarArcLength, &["t"]);
    assert!(line_integral(&mut engine, &invalid_line).is_err());

    let mut invalid_surface = surface_request(SurfaceIntegralKind::ScalarArea, &["1"]);
    invalid_surface.upper[0] = "v".into();
    assert!(surface_integral(&mut engine, &invalid_surface).is_err());
}
