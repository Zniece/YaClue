use processing::elaboration::elaborate_input;

#[test]
fn accepts_representative_frontend_yaclue_output() {
    let cases = [
        "1+2*x^2",
        "Sin(x)+Gamma(x)",
        "{{1,2},{3,4}}",
        "Dot({1,2},{3,4})",
        "D(x)(x^2)",
        "Integrate(x,0,1)(x^2)",
        "Limit(x,0,Left)(1/x)",
        "Taylor(x,0,2)(Exp(x))",
        "Subst(x,2)(x^2)",
        "DoubleIntegral(x+y,x,0,1,y,0,2)",
        "Gradient(x^2+y^2,{x,y})",
        "Solve(x==1,x)",
        "Plot(x^2,x,0,1)",
    ];

    for source in cases {
        elaborate_input(source)
            .unwrap_or_else(|error| panic!("frontend output {source:?} was rejected: {error}"));
    }
}

#[test]
fn accepts_every_keyboard_generated_yaclue_expression() {
    // Generated from the filled key templates in ml-demo/demo.html and kept
    // named so a contract failure identifies the exact key that drifted.
    let cases = [
        ("add", "1+2"),
        ("subtract", "1-2"),
        ("multiply", "(x)*(1)"),
        ("equation", "(x)==(1)"),
        ("decimal", "1.5"),
        ("fraction", "(x)/(1)"),
        ("power", "x^(1)"),
        ("square", "x^(2)"),
        ("root", "Sqrt(1)"),
        ("absolute", "Abs(1)"),
        ("norm", "Norm(1)"),
        ("sin", "Sin(1)"),
        ("cos", "Cos(1)"),
        ("tan", "Tan(1)"),
        ("ln", "Ln(1)"),
        ("exp", "Exp(1)"),
        ("arcsin", "ArcSin(1)"),
        ("arccos", "ArcCos(1)"),
        ("arctan", "ArcTan(1)"),
        ("floor", "Floor(1)"),
        ("ceil", "Ceil(1)"),
        ("gamma", "Gamma(1)"),
        ("zeta", "Zeta(1)"),
        ("lambertW", "LambertW(1)"),
        ("derivative", "D(x)(1)"),
        ("nthDerivative", "D(x,1)(1)"),
        ("integral", "Integrate(x)(1)"),
        ("definiteIntegral", "Integrate(x,1,1)(1)"),
        ("limit", "Limit(x,1)(1)"),
        ("sum", "Sum(k,1,1,1)"),
        ("comma", "{1,2}"),
        ("list", "{1}"),
        ("vector", "{1,1}"),
        ("matrix", "{{1}}"),
        ("sign", "Sign(1)"),
        ("round", "Round(1)"),
        ("min", "Min(1,1)"),
        ("max", "Max(1,1)"),
        ("div", "Div(1,1)"),
        ("mod", "Mod(1,1)"),
        ("gcd", "Gcd(1,1)"),
        ("lcm", "Lcm(1,1)"),
        ("numer", "Numer(1)"),
        ("denom", "Denom(1)"),
        ("bernoulli", "Bernoulli(1)"),
        ("euler", "Euler(1)"),
        ("leftLimit", "Limit(x,1,Left)(1)"),
        ("rightLimit", "Limit(x,1,Right)(1)"),
        ("taylor", "Taylor(x,1,1)(1)"),
        ("substitute", "Subst(x,1)(1)"),
        ("doubleIntegral", "DoubleIntegral(1,x,1,1,y,1,1)"),
        ("polarIntegral", "PolarIntegral(1,x,y,r,theta,1,1,1,1)"),
        ("infinity", "Infinity"),
        ("principalValue", "PrincipalValueIntegral(1,x,1,1)"),
        ("partial", "D(x)(1)"),
        ("gradient", "Gradient(1,1)"),
        ("jacobian", "Jacobian(1,1)"),
        ("hessian", "Hessian(1,1)"),
        ("divergence", "Divergence(1,1)"),
        ("curl", "Curl(1,1)"),
        ("directional", "DirectionalDerivative(1,1,1)"),
        ("scalarLine", "ScalarLineIntegral(1,1,1,t,1,1)"),
        ("vectorLine", "VectorLineIntegral(1,1,1,t,1,1)"),
        ("scalarSurface", "ScalarSurfaceIntegral(1,1,1,1,1,1)"),
        ("vectorSurface", "VectorSurfaceIntegral(1,1,1,1,1,1)"),
        ("dot", "Dot(1,1)"),
        ("cross", "CrossProduct(1,1)"),
        ("outer", "Outer(1,1)"),
        ("normalize", "Normalize(1)"),
        ("pnorm", "PNorm(1,1)"),
        ("transpose", "Transpose(1)"),
        ("determinant", "Determinant(1)"),
        ("inverse", "Inverse(1)"),
        ("rank", "Rank(1)"),
        ("rref", "RREF(1)"),
        ("trace", "Trace(1)"),
        ("eigen", "EigenValues(1)"),
        ("nullSpace", "NullSpace(1)"),
        ("matrixSolve", "MatrixSolve(1,1)"),
        ("rowReduce", "RowReduce(1)"),
        ("columnSpace", "ColumnSpace(1)"),
        ("eigenSpaces", "EigenSpaces(1)"),
        ("pldu", "PLDU(1)"),
        ("cholesky", "Cholesky(1)"),
        ("gramSchmidt", "GramSchmidt(1)"),
        ("orthogonal", "OrthogonalBasis(1)"),
        ("orthonormal", "OrthonormalBasis(1)"),
        ("factors", "Factors(1)"),
        ("matrixPower", "MatrixPower(1,1)"),
        ("diagonal", "Diagonal(1)"),
        ("identity", "Identity(1)"),
        ("factor", "Factor(1)"),
        ("expand", "Expand(1)"),
        ("simplify", "Simplify(1)"),
        ("tidy", "Tidy(1)"),
        ("apart", "Apart(1,x)"),
        ("solve", "Solve(1,x)"),
        ("numeric", "N(1)"),
        ("findRoot", "FindRoot(1,x,1)"),
        ("ode", "OdeSolve(1)"),
        ("plot", "Plot(1,x,1,1)"),
        ("extrema", "Extrema(1,x,y)"),
        ("lagrange", "Lagrange(1,1,x,y)"),
    ];

    for (key, source) in cases {
        elaborate_input(source).unwrap_or_else(|error| {
            panic!("keyboard key {key:?} generated rejected YaClue {source:?}: {error}")
        });
    }
}

#[test]
fn accepts_frontend_partial_applications() {
    for source in [
        "D(x)",
        "D(x,2)",
        "Integrate(x)",
        "Integrate(x,0,1)",
        "Sum(k,1,n)",
        "Subst(x,2)",
    ] {
        elaborate_input(source).unwrap_or_else(|error| {
            panic!("frontend partial application {source:?} was rejected: {error}")
        });
    }
}

#[test]
fn preserves_frontend_ascii_identifier_and_function_names() {
    for source in [
        "mass+Mass+value2+X99",
        "2*value2+mass^2",
        "foo(x)+Foo(x)+foo2(x+1)",
        "outer(inner(x))",
        "sin(x)+Sin(x)",
    ] {
        elaborate_input(source).unwrap_or_else(|error| {
            panic!("frontend identifier expression {source:?} was rejected: {error}")
        });
    }
}
