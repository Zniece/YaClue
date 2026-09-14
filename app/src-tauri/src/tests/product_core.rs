use super::*;

#[test]
fn every_registered_operator_family_reaches_the_structured_product_exit() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let cases = [
        ("D(x)(x^2)", "D"),
        ("Factor(x^2-1)", "Factor"),
        ("Expand((x+1)^2)", "Expand"),
        ("Apart((x+1)/(x^2-1),x)", "Apart"),
        ("Integrate(x)(x)", "Integrate"),
        ("Subst(x,2)(x^2+1)", "Subst"),
        ("Limit(x,0)(Sin(x)/x)", "Limit"),
        ("Taylor(Exp(x),0,2)", "Taylor"),
        ("Solve(x^2==1,x)", "Solve"),
        ("Transpose({{1,2},{3,4}})", "Transpose"),
        ("MatrixSolve({{1,0},{0,1}},{2,3})", "MatrixSolve"),
        ("Rank({{1,2},{2,4}})", "Rank"),
        ("PLDU({{4,2},{2,2}})", "PLDU"),
        ("Factors(PLDU({{4,2},{2,2}}))", "Factors"),
        ("OdeSolve(y'==y)", "OdeSolve"),
        ("N(Pi,12)", "N"),
        ("Sum(k,1,3,k)", "Sum"),
        ("ImproperIntegral(Exp(-x),x,0,Infinity)", "ImproperIntegral"),
        (
            "PrincipalValueIntegral(1/x,x,-1,1,{0})",
            "PrincipalValueIntegral",
        ),
        ("DoubleIntegral(x+y,y,0,2,x,0,1)", "DoubleIntegral"),
        ("PolarIntegral(x^2+y^2,x,y,r,t,0,1,0,2*Pi)", "PolarIntegral"),
        ("OdeSolveNumeric(y'==y,x,y,0,1,0.1)", "OdeSolveNumeric"),
        ("FindRoot(x^2-2,x,1)", "FindRoot"),
        ("Plot(x^2,x,0,1)", "Plot"),
        ("Extrema(x^2+y^2,x,y)", "Extrema"),
        ("Lagrange(x+y,x^2+y^2-1,x,y)", "Lagrange"),
        ("Gradient(x^2+y^2,{x,y})", "Gradient"),
        (
            "DirectionalDerivative(x^2+y^2,{x,y},{1,0})",
            "DirectionalDerivative",
        ),
        (
            "ScalarLineIntegral(x,{x,y},{t,0},t,0,1)",
            "ScalarLineIntegral",
        ),
        (
            "ScalarSurfaceIntegral(1,{x,y,z},{u,v,0},{u,v},{0,0},{1,1})",
            "ScalarSurfaceIntegral",
        ),
    ];
    let covered = cases.iter().map(|(_, name)| *name).collect::<BTreeSet<_>>();
    let registered = processing::semantic_core::OPERATOR_DESCRIPTORS
        .iter()
        .map(|descriptor| descriptor.names[0])
        .collect::<BTreeSet<_>>();
    assert_eq!(covered, registered, "update the D3 acceptance matrix");

    for (source, name) in cases {
        let result = process_expression_with_engine(request(source, false), &mut engine)
            .unwrap_or_else(|error| panic!("{name} ({source}): {}", error.message));
        assert!(!result.expression.is_empty(), "{name}");
        assert!(!result.tex.is_empty(), "{name}");
        assert!(result.steps.is_empty(), "{name}");
        assert!(result.status.is_some(), "{name}");
        assert_eq!(
            result.outcome.support,
            processing::protocol::SupportState::Supported,
            "{name}"
        );
    }
}

#[test]
fn unified_expression_dispatches_core_calculator_paths() {
    let mut engine = RustEngineProxy::spawn().unwrap();

    let derivative =
        process_expression_with_engine(request("D(x)Sin(x)^2", true), &mut engine).unwrap();
    assert_eq!(derivative.kind, "derivative");
    assert!(!derivative.steps.is_empty());
    assert!(derivative.steps.iter().all(|step| {
        step.message_ref.key == format!("steps.{}", step.rule)
            && step.message_ref.fallback.is_none()
    }));

    let limit =
        process_expression_with_engine(request("Limit(x,0)(Sin(x)/x)", true), &mut engine).unwrap();
    assert!(limit.steps.iter().all(|step| {
        step.message_ref.args.get("variable").map(String::as_str) == Some("x")
            && step.message_ref.args.get("at").map(String::as_str) == Some("0")
    }));
    assert_eq!(derivative.semantic.symbols, ["x"]);
    assert!(derivative.semantic.bound_symbols.is_empty());

    let composed =
        process_expression_with_engine(request("D(x)Integrate(x)x*Exp(x)", true), &mut engine)
            .unwrap();
    assert_eq!(composed.kind, "composition");
    assert!(composed
        .steps
        .iter()
        .any(|step| step.rule == "derivative-of-indefinite-integral"));
    assert!(composed
        .semantic
        .symbol_identities
        .iter()
        .all(|identity| identity.role != SymbolRole::ArbitraryConstant));
    assert!(composed
        .steps
        .iter()
        .all(|step| step.rule != "antiderivative-family"));

    let repeated_integral =
        process_expression_with_engine(request("Integrate(x)Integrate(x)x", true), &mut engine)
            .unwrap();
    assert_eq!(repeated_integral.kind, "composition");
    assert!(repeated_integral.expression.contains('C'));
    assert!(repeated_integral.expression.contains("C1"));
    assert_eq!(
        repeated_integral
            .semantic
            .symbol_identities
            .iter()
            .filter(|identity| identity.role == SymbolRole::ArbitraryConstant)
            .count(),
        2
    );

    let matrix =
        process_expression_with_engine(request("{{1,2},{3,4}}*{{5,6},{7,8}}", false), &mut engine)
            .unwrap();
    assert_eq!(matrix.kind, "matrix");
    assert!(matrix.expression.contains("19"));
    assert_eq!(
        matrix.semantic.shape,
        Some(processing::semantic::MatrixShape {
            rows: 2,
            columns: 2
        })
    );

    let ode =
        process_expression_with_engine(request("OdeSolve(y'==y)", true), &mut engine).unwrap();
    assert_eq!(ode.kind, "ode");
    assert!(!ode.steps.is_empty());
    assert!(!ode.expression.contains("C7"));
    assert!(ode.semantic.symbols.is_empty());
    assert_eq!(ode.semantic.bound_symbols, ["x"]);
    assert!(ode.semantic.symbol_identities.iter().any(|identity| {
        identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
    }));

    let oscillatory =
        process_expression_with_engine(request("OdeSolve(y''+2*y'+5*y==0)", true), &mut engine)
            .unwrap();
    assert!(!oscillatory.expression.contains("Complex("));
    assert!(oscillatory.expression.contains("Cos"));
    assert_eq!(
        oscillatory.status,
        Some(processing::composition::CompositionStatus::Completed)
    );
    assert!(oscillatory
        .operators
        .contains(&processing::semantic_core::OperatorId::OdeSolve));
    assert!(!oscillatory
        .semantic
        .symbol_identities
        .iter()
        .any(|identity| identity.name == "y" || identity.name.starts_with("y'")));

    let equations =
        process_expression_with_engine(request("Solve({x+y==3,x-y==1},{x,y})", false), &mut engine)
            .unwrap();
    assert_eq!(equations.kind, "equation");
    assert!(!equations.tex.is_empty());
    assert_eq!(
        equations.semantic.kind,
        processing::semantic::ValueKind::SolutionSet
    );

    let double_integral = process_expression_with_engine(
        request("DoubleIntegral(x+y,y,0,x,x,0,1)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(double_integral.kind, "double_integral");
    assert!(!double_integral.steps.is_empty());

    let improper = process_expression_with_engine(
        request("Integrate(x,0,Infinity)Exp(-x)", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(improper.kind, "integral");
    assert_eq!(improper.expression, "1");

    let parameterized_gamma = process_expression_with_engine(
        request("Integrate(t,0,Infinity)t^(1/x-1)*Exp(-t)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(parameterized_gamma.kind, "integral");
    assert_eq!(
        parameterized_gamma.outcome.conditionality,
        processing::protocol::Conditionality::Conditional
    );

    let explicit_gamma = process_expression_with_engine(
        request("ImproperIntegral(t^(a-1)*Exp(-t),t,0,Infinity)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(explicit_gamma.kind, "defined_object");
    assert!(explicit_gamma.expression.contains("Gamma"));
    assert_eq!(explicit_gamma.semantic.symbols, ["a".to_string()]);
    assert!(explicit_gamma.semantic.bound_symbols.is_empty());
    assert_eq!(
        explicit_gamma.outcome.conditionality,
        processing::protocol::Conditionality::Conditional
    );

    let symbolic_integral = process_expression_with_engine(
        request("Integrate(x)(theta+theta1)*x^2/Sqrt(4-x^2)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(symbolic_integral.kind, "integral");
    assert!(!symbolic_integral.expression.is_empty());
    assert!(!symbolic_integral.tex.is_empty());
    assert!(!symbolic_integral.steps.is_empty());
    assert_eq!(
        symbolic_integral.semantic.symbols,
        ["theta".to_string(), "theta1".to_string()]
    );
    assert_eq!(symbolic_integral.semantic.bound_symbols, ["x".to_string()]);
    assert_eq!(symbolic_integral.semantic.kind, ValueKind::FunctionFamily);
    assert!(
        symbolic_integral.expression.replace(' ', "").contains("+C"),
        "{}",
        symbolic_integral.expression
    );
    assert_eq!(
        symbolic_integral.steps.last().unwrap().rule,
        "antiderivative-family"
    );
    assert_eq!(
        symbolic_integral.status,
        Some(processing::composition::CompositionStatus::Completed)
    );
    assert!(symbolic_integral
        .semantic
        .symbol_identities
        .iter()
        .any(|identity| identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant));

    let colliding_constant =
        process_expression_with_engine(request("Integrate(x)C*x", false), &mut engine).unwrap();
    assert_eq!(colliding_constant.kind, "integral");
    assert!(colliding_constant.expression.contains("C1"));
    assert_eq!(colliding_constant.semantic.symbols, ["C"]);
    assert!(colliding_constant
        .semantic
        .symbol_identities
        .iter()
        .any(|identity| {
            identity.name == "C1" && identity.role == SymbolRole::ArbitraryConstant
        }));

    let direct_gamma =
        process_expression_with_engine(request("Gamma(3)", false), &mut engine).unwrap();
    assert_eq!(direct_gamma.kind, "evaluation");
    assert_eq!(direct_gamma.expression, "2");

    let polar_template = process_expression_with_engine(
        request("PolarIntegral(x^2+y^2,x,y,r,t,0,1,0,2*Pi)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(polar_template.kind, "polar_integral");
    assert_eq!(
        polar_template.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
    assert!(!polar_template.steps.is_empty());

    let gaussian_disk = process_expression_with_engine(
        request(
            "PolarIntegral(Exp(-(x^2+y^2)),x,y,r,theta,0,2,0,2*Pi)",
            true,
        ),
        &mut engine,
    )
    .unwrap();
    assert_eq!(gaussian_disk.kind, "polar_integral");
    assert_eq!(
        gaussian_disk.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
    assert_eq!(
        gaussian_disk.status,
        Some(processing::composition::CompositionStatus::Completed)
    );
    assert!(gaussian_disk
        .steps
        .iter()
        .any(|step| step.rule == "polar-transform-integrand"));

    let principal_value = process_expression_with_engine(
        request("PrincipalValueIntegral(1/x,x,-1,1,{0})", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(principal_value.expression, "0");

    let divergent = process_expression_with_engine(
        request("ImproperIntegral(1/x,x,-1,1,{0})", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(
        divergent.outcome.reason,
        Some(processing::protocol::OutcomeReason::Divergent)
    );
}

#[test]
fn unified_plain_inputs_expose_the_structured_computation_exit() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for (source, kind, expected) in [
        ("Gamma(3)", "evaluation", "2"),
        ("x^2==1", "equation", "x^2==1"),
        ("{1,Sin(x)}", "evaluation", "{1,Sin(x)}"),
        ("3", "evaluation", "3"),
    ] {
        let result = process_expression_with_engine(request(source, false), &mut engine)
            .unwrap_or_else(|error| panic!("{source}: {}", error.message));
        assert_eq!(result.kind, kind, "{source}");
        assert_eq!(result.expression.replace(' ', ""), expected, "{source}");
        assert_eq!(
            result.status,
            Some(processing::composition::CompositionStatus::Completed),
            "{source}"
        );
        assert!(!result.effect_only, "{source}");
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Solved,
            "{source}"
        );
    }
}

#[test]
fn product_session_preserves_assumptions_and_recovers_after_invalid_input() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    processing::assumptions::assume(
        &mut engine,
        "x",
        processing::assumptions::AssumptionFact::Positive,
    )
    .unwrap();

    let assumed =
        process_expression_with_engine(request("Simplify(Sqrt(x^2))", false), &mut engine).unwrap();
    assert_eq!(assumed.expression, "x");

    assert!(process_expression_with_engine(request("D(x", false), &mut engine).is_err());
    let recovered =
        process_expression_with_engine(request("D(x)(x^2)", false), &mut engine).unwrap();
    assert_eq!(recovered.expression, "2*x");

    processing::assumptions::clear_assumptions(&mut engine).unwrap();
    let cleared =
        process_expression_with_engine(request("Simplify(Sqrt(x^2))", false), &mut engine).unwrap();
    assert_ne!(cleared.expression, "x");
}

#[test]
fn remaining_operator_families_emit_translated_product_messages() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let catalogue = include_str!("../../../src/i18n.js");
    let cases = [
        "Factor(x^2-1)",
        "Expand((x+1)^2)",
        "Apart((x+1)/(x^2-1),x)",
        "Subst(x,2)(x^2+1)",
        "Taylor(Exp(x),0,3)",
        "Determinant({{1,2},{3,4}})",
        "Inverse({{1,2},{3,4}})",
        "EigenValues({{2,1},{1,2}})",
        "Transpose({{1,2},{3,4}})",
        "Rank({{1,2},{2,4}})",
        "MatrixSolve({{1,0},{0,1}},{2,3})",
        "PLDU({{4,2},{2,2}})",
        "Factors(PLDU({{4,2},{2,2}}))",
        "DoubleIntegral(x+y,y,0,2,x,0,1)",
        "PolarIntegral(x^2+y^2,x,y,r,t,0,1,0,2*Pi)",
        "Gradient(x^2+y^2,{x,y})",
        "DirectionalDerivative(x^2+y^2,{x,y},{1,0})",
        "ScalarLineIntegral(x,{x,y},{t,0},t,0,1)",
        "VectorLineIntegral({y,x},{x,y},{t,t^2},t,0,1)",
        "ScalarSurfaceIntegral(1,{x,y,z},{u,v,0},{u,v},{0,0},{1,1})",
        "VectorSurfaceIntegral({0,0,1},{x,y,z},{u,v,0},{u,v},{0,0},{2,3},Reversed)",
        "Sum(k,1,3,k)",
        "Sum(k,1,Infinity,1/k^2)",
        "N(Pi,12)",
        "OdeSolveNumeric(y'==y,x,y,0,1,0.1)",
        "FindRoot(x^2-2,x,1)",
        "Plot(x^2,x,0,1)",
        "Extrema(x^2+y^2,x,y)",
        "Lagrange(x+y,x^2+y^2-1,x,y)",
        "ImproperIntegral(Exp(-x),x,0,Infinity)",
        "ImproperIntegral(1/x,x,1,Infinity)",
        "PrincipalValueIntegral(1/x,x,-1,1,{0})",
    ];
    let mut missing = BTreeSet::new();
    for source in cases {
        let result = process_expression_with_engine(request(source, true), &mut engine)
            .unwrap_or_else(|error| panic!("{source}: {}", error.message));
        let keys = result
            .steps
            .iter()
            .map(|step| &step.message_ref.key)
            .chain(result.analyses.iter().map(|item| &item.message_ref.key))
            .chain(result.conclusions.iter().map(|item| &item.message_ref.key));
        for key in keys {
            if catalogue.matches(&format!("\"{key}\"")).count() != 2 {
                missing.insert(key.clone());
            }
        }
    }
    assert!(missing.is_empty(), "missing locale keys: {missing:#?}");
}

#[test]
fn mathematical_message_wire_format_contains_only_keys_and_arguments() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for source in ["D(x)Sin(x)^2", "Limit(Sin(x)/x,0)", "Plot(x^2,x,0,1)"] {
        let result = process_expression_with_engine(request(source, true), &mut engine).unwrap();
        let value = serde_json::to_value(result).unwrap();
        assert!(value.get("title").is_none(), "{source}: {value}");
        for section in ["steps", "analyses", "conclusions"] {
            for item in value[section].as_array().unwrap() {
                assert!(item.get("why").is_none(), "{source}: {item}");
                assert!(item.get("message").is_none(), "{source}: {item}");
                let message = item["message_ref"].as_object().unwrap();
                assert!(message.get("key").is_some(), "{source}: {item}");
                assert!(message.get("fallback").is_none(), "{source}: {item}");
            }
        }
    }
}

#[test]
fn user_errors_are_keyed_and_do_not_serialize_backend_diagnostics() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let error = match process_expression_with_engine(
        ProcessExpressionRequest {
            expression: "x+1".into(),
            steps: true,
            verbosity: "exhaustive".into(),
        },
        &mut engine,
    ) {
        Ok(_) => panic!("unknown verbosity must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.message_ref.key, "errors.unknown_verbosity");
    assert_eq!(
        error.message_ref.args.get("value").map(String::as_str),
        Some("exhaustive")
    );
    let wire = serde_json::to_value(error).unwrap();
    assert!(wire.get("message").is_none());
    assert!(wire["message_ref"].get("fallback").is_none());

    let assumption = parse_assumption_fact("complex").unwrap_err();
    assert_eq!(assumption.message_ref.key, "errors.unknown_assumption_fact");
    assert_eq!(
        assumption.message_ref.args.get("value").map(String::as_str),
        Some("complex")
    );
}
