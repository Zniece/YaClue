use processing::engine::{Engine, RustEngine};

struct CapabilityCase {
    name: &'static str,
    command: &'static str,
    expected: &'static str,
}

const CASES: &[CapabilityCase] = &[
    CapabilityCase {
        name: "partial-derivative",
        command: "Deriv(x)(x^2*y+Sin(y))",
        expected: "(2 * (x * y))",
    },
    CapabilityCase {
        name: "gradient-components",
        command: "{Deriv(x)(x^2*y+Sin(y)),Deriv(y)(x^2*y+Sin(y))}",
        expected: "List((2 * (x * y)),((x ^ 2) + Cos(y)))",
    },
    CapabilityCase {
        name: "rectangular-jacobian",
        command: "JacobianMatrix({x+y,x*y,x^2},{x,y})",
        expected: "List(List(1,1),List(y,x),List((2 * x),0))",
    },
    CapabilityCase {
        name: "hessian",
        command: "HessianMatrix(x^2*y+y^3,{x,y})",
        expected: "List(List((2 * y),(2 * x)),List((2 * x),(6 * y)))",
    },
    CapabilityCase {
        name: "divergence",
        command: "Diverge({x^2,y^2,z^2},{x,y,z})",
        expected: "(((2 * x) + (2 * y)) + (2 * z))",
    },
    CapabilityCase {
        name: "curl",
        command: "Curl({y*z,x*z,x*y},{x,y,z})",
        expected: "List(0,0,0)",
    },
];

#[test]
fn multivariate_differential_capability_matrix() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    for case in CASES {
        let result = engine
            .eval(case.command)
            .unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
        assert_eq!(result.expr.to_string(), case.expected, "{}", case.name);
    }
}

#[test]
fn invalid_curl_dimension_does_not_poison_the_engine() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    assert!(engine.eval("Curl({x,y},{x,y})").is_err());
    assert_eq!(
        engine.eval("Deriv(x)(x^2)").unwrap().expr.to_string(),
        "(2 * x)"
    );
}
