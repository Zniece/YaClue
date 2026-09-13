use processing::composition::execute_steps;
use processing::engine::{Engine, RustEngine};
use processing::multiple_integrals::{
    double_integral, double_integral_steps, polar_integral, polar_integral_steps, triple_integral,
    triple_integral_steps, IntegralBound, IteratedIntegralStatus, PolarRegion,
    TripleIntegralStatus,
};
use processing::steps::StepVerbosity;

fn assert_equivalent(engine: &mut dyn Engine, actual: &str, expected: &str) {
    let check = engine
        .eval(&format!("IsZero(Simplify(({actual})-({expected})))"))
        .unwrap();
    assert_eq!(check.expr.to_string(), "True", "{actual} != {expected}");
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

    let line = execute_steps(
        &mut engine,
        "VectorLineIntegral({y,x},{x,y},{t,t^2},t,0,1)",
        StepVerbosity::Detailed,
    )
    .unwrap()
    .unwrap();
    assert_equivalent(&mut engine, &line.value, "1");
    assert_eq!(line.steps.last().unwrap().expr, line.value);

    let surface = execute_steps(
        &mut engine,
        "VectorSurfaceIntegral({0,0,1},{x,y,z},{u,v,u+v},{u,v},{0,0},{1,1})",
        StepVerbosity::Detailed,
    )
    .unwrap()
    .unwrap();
    assert_equivalent(&mut engine, &surface.value, "1");
    assert_eq!(surface.steps.last().unwrap().expr, surface.value);
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

    assert!(execute_steps(
        &mut engine,
        "ScalarLineIntegral(t,{x,y},{t,t^2},t,0,1)",
        StepVerbosity::Concise,
    )
    .is_err());
    assert!(execute_steps(
        &mut engine,
        "ScalarSurfaceIntegral(1,{x,y,z},{u,v,u+v},{u,v},{0,0},{v,1})",
        StepVerbosity::Concise,
    )
    .is_err());
}
