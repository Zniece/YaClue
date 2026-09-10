use processing::engine::{Engine, EngineError, RustEngine};

struct CapabilityCase {
    name: &'static str,
    command: &'static str,
    expected: &'static str,
}

const CASES: &[CapabilityCase] = &[
    CapabilityCase {
        name: "gram-schmidt",
        command: "OrthogonalBasis({{1,1,0},{2,0,1},{2,2,1}})",
        expected: "List(List(1,1,0),List(1,-1,1),List((-1 / 3),(1 / 3),(2 / 3)))",
    },
    CapabilityCase {
        name: "lu-reconstruction",
        command: "[Local(a,l,u); a:={{2,1,1},{2,2,-1},{4,-1,6}}; {l,u}:=LU(a); Simplify(l*u-a);]",
        expected: "List(List(0,0,0),List(0,0,0),List(0,0,0))",
    },
    CapabilityCase {
        name: "pivoted-ldu-reconstruction",
        command: "[Local(a,p,l,d,u); a:={{0,1},{1,0}}; {p,l,d,u}:=PLDU(a); Simplify(p*a-l*d*u);]",
        expected: "List(List(0,0),List(0,0))",
    },
    CapabilityCase {
        name: "cholesky-reconstruction",
        command: "[Local(a,r); a:={{4,2},{2,3}}; r:=Cholesky(a); Simplify(Transpose(r)*r-a);]",
        expected: "List(List(0,0),List(0,0))",
    },
    CapabilityCase {
        name: "symbolic-eigenvalues",
        command: "EigenValues({{1,2},{3,4}})",
        expected: "List(((Sqrt(33) + 5) / 2),((5 - Sqrt(33)) / 2))",
    },
    CapabilityCase {
        name: "linear-system-residual",
        command:
            "[Local(a,b,x); a:={{2,1},{1,-1}}; b:={5,1}; x:=MatrixSolve(a,b); Simplify(a*x-b);]",
        expected: "List(0,0)",
    },
];

#[test]
fn linear_algebra_capability_matrix() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    for case in CASES {
        let result = engine
            .eval(case.command)
            .unwrap_or_else(|error| panic!("{} failed: {error}", case.name));
        assert_eq!(result.expr.to_string(), case.expected, "{}", case.name);
    }
}

#[test]
fn orthogonal_basis_rejects_dependent_and_zero_leading_vectors() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    for vectors in ["{{1,0},{2,0}}", "{{0,0},{1,0}}"] {
        assert!(matches!(
            engine.eval(&format!("OrthogonalBasis({vectors})")),
            Err(EngineError::InvalidInput(_))
        ));
    }
}

#[test]
fn invalid_decomposition_input_does_not_poison_the_engine() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    assert!(engine.eval("Cholesky({{1,2},{2,1}})").is_err());
    assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
}

#[test]
fn eigenvectors_have_a_product_usable_vector_contract() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    let result = engine
        .eval("EigenVectors({{2,0},{0,3}},{2,3})")
        .expect("diagonal matrix eigenvectors");
    assert_eq!(result.expr.to_string(), "List(List(1,0),List(0,1))");
    assert_eq!(
        engine
            .eval("{Simplify({{2,0},{0,3}}*{1,0}-2*{1,0}),Simplify({{2,0},{0,3}}*{0,1}-3*{0,1})}")
            .unwrap()
            .expr
            .to_string(),
        "List(List(0,0),List(0,0))"
    );
}
