use super::*;

#[test]
fn unified_input_accepts_two_argument_limit_with_default_x() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for steps in [true, false] {
        let result =
            process_expression_with_engine(request("Limit(x,0)", steps), &mut engine).unwrap();
        assert_eq!(result.kind, "limit");
        assert_eq!(result.expression, "0");
        assert_eq!(result.steps.is_empty(), !steps);
        assert!(result.semantic.bound_symbols.is_empty());
        assert!(result.semantic.symbols.is_empty());
        assert_eq!(
            result.outcome.support,
            processing::protocol::SupportState::Supported
        );
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Solved
        );
    }
}

#[test]
fn unified_equation_no_solution_is_a_structured_conclusion() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let result =
        process_expression_with_engine(request("Solve(Sqrt(x)==-1,x)", true), &mut engine).unwrap();
    assert_eq!(result.expression, "NoSolutions({x})");
    assert!(result
        .steps
        .iter()
        .all(|step| step.rule != "solve-no-solution"));
    assert!(!result.conclusions.is_empty());
    assert_eq!(
        result.conclusions[0].message,
        "方程或方程组没有满足条件的解。"
    );
}

#[test]
fn semantic_calculus_end_to_end_acceptance() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let started = std::time::Instant::now();
    let cases = [
        ("Limit(t,0)(Sin(t)/t+x^2)", "x^2+1", "limit"),
        (
            "D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t)))",
            "Gamma(x)*PolyGamma(0,x)",
            "composition",
        ),
        (
            "D(x)(Integrate(t,0,1)(Sin(x*t)/(1+t^2)))",
            "Integrate(t,0,1)D(x,1)Sin(x*t)/(1+t^2)",
            "composition",
        ),
    ];
    for (source, expected, kind) in cases {
        let without_steps =
            process_expression_with_engine(request(source, false), &mut engine).unwrap();
        let with_steps =
            process_expression_with_engine(request(source, true), &mut engine).unwrap();
        assert_eq!(without_steps.expression, expected, "{source}");
        assert_eq!(with_steps.expression, expected, "{source}");
        assert_eq!(without_steps.kind, kind, "{source}");
        assert!(without_steps.steps.is_empty(), "{source}");
        assert!(!with_steps.steps.is_empty(), "{source}");
        assert_eq!(without_steps.outcome, with_steps.outcome, "{source}");
        let line = serde_json::to_string(&with_steps).unwrap();
        assert!(!line.contains('\n'));
        let decoded: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(decoded["expression"], expected);
    }
    let gamma = process_expression_with_engine(
        request("D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t)))", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(gamma.outcome.conditionality, Conditionality::Conditional);
    assert!(gamma
            .outcome
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, Condition::RealPartPositive { expression } if expression == "x")));
    let held = process_expression_with_engine(
        request("D(x)(Integrate(t,0,1)(Sin(x*t)/(1+t^2)))", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(held
        .outcome
        .conditions
        .conditions()
        .iter()
        .any(|condition| matches!(condition, Condition::Unknown { .. })));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "semantic calculus acceptance exceeded its bounded runtime"
    );
}

#[test]
fn unified_input_accepts_three_argument_taylor_with_default_x() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for steps in [true, false] {
        let result =
            process_expression_with_engine(request("Taylor(Exp(x),0,6)", steps), &mut engine)
                .unwrap();
        assert_eq!(result.kind, "composition");
        assert!(
            result.expression.contains("x ^ 6") || result.expression.contains("x^6"),
            "{}",
            result.expression
        );
        assert_eq!(result.steps.is_empty(), !steps);
        assert!(result.semantic.bound_symbols.is_empty());
        assert_eq!(result.semantic.symbols, ["x"]);
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Solved
        );
    }
}

#[test]
fn unified_equation_lowers_operands_without_implicitly_solving() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let result = process_expression_with_engine(
        request("y'==(Integrate(x)Taylor(Exp(x),0,2))", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(result.kind, "equation");
    assert!(
        !result.expression.contains("Integrate"),
        "{}",
        result.expression
    );
    assert!(
        !result.expression.contains("Taylor"),
        "{}",
        result.expression
    );
    assert!(result.expression.contains("C"), "{}", result.expression);
    assert!(result.expression.contains("y'"), "{}", result.expression);
    assert!(result.steps.iter().any(|step| step.rule == "taylor-expand"));
    assert!(result
        .steps
        .iter()
        .any(|step| step.rule == "antiderivative-family"));
    assert!(result.semantic.bound_symbols.is_empty());
    assert!(result.semantic.symbols.contains(&"x".into()));
    assert!(result.semantic.symbol_identities.iter().any(|identity| {
        identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
    }));
    assert_eq!(
        result.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
}

#[test]
fn unified_ode_composition_uses_normalized_solutions_and_result_semantics() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let first =
        process_expression_with_engine(request("D(x)OdeSolve(y'==y)", true), &mut engine).unwrap();
    assert_eq!(first.kind, "composition");
    assert!(
        !first.expression.contains("C179"),
        "{:#?}",
        first.expression
    );
    assert!(!first
        .semantic
        .symbols
        .iter()
        .any(|name| name.starts_with('y')));
    assert_eq!(first.semantic.symbols, ["x"]);
    assert!(first.semantic.bound_symbols.is_empty());
    assert!(first.semantic.symbol_identities.iter().any(|identity| {
        identity.name == "C" && identity.role == SymbolRole::ArbitraryConstant
    }));

    let second =
        process_expression_with_engine(request("D(x)OdeSolve(y''+4*y==Sin(x))", true), &mut engine)
            .unwrap();
    assert_eq!(second.kind, "composition");
    assert!(
        !second.expression.contains("Deriv(x,y"),
        "{}",
        second.expression
    );
    assert!(!second.expression.contains("y(2)"), "{}", second.expression);
    assert!(!second
        .semantic
        .symbols
        .iter()
        .any(|name| name.starts_with('y')));

    let transformed =
        process_expression_with_engine(request("D(x)Simplify((x+x)/2)", true), &mut engine)
            .unwrap();
    assert_eq!(transformed.kind, "composition");
    assert_eq!(transformed.expression, "1");
    assert!(transformed.steps.iter().any(|step| matches!(
        step.rule.as_str(),
        "apply-algebra-transform" | "confirm-algebra-normal-form"
    )));

    let apart =
        process_expression_with_engine(request("Apart((x+1)/(x^2-1),x)", true), &mut engine)
            .unwrap();
    assert_eq!(apart.kind, "algebra");
    assert!(!apart.expression.contains("List"), "{}", apart.expression);

    let limited =
        process_expression_with_engine(request("D(x)Limit(t,0)(Sin(t)/t+x^2)", true), &mut engine)
            .unwrap();
    assert_eq!(limited.kind, "composition");
    assert_eq!(limited.expression, "2*x");
    assert_eq!(limited.semantic.symbols, ["x"]);
    assert!(limited.semantic.bound_symbols.is_empty());

    let limited_without_steps =
        process_expression_with_engine(request("D(x)Limit(t,0)(Sin(t)/t+x^2)", false), &mut engine)
            .unwrap();
    assert_eq!(limited_without_steps.kind, "composition");
    assert_eq!(limited_without_steps.expression, "2*x");
    assert!(limited_without_steps.steps.is_empty());
    assert_eq!(limited_without_steps.semantic.symbols, ["x"]);
    assert!(limited_without_steps.semantic.bound_symbols.is_empty());

    let absent =
        process_expression_with_engine(request("D(x)Limit(t,0)(1/t)", false), &mut engine).unwrap();
    assert_eq!(absent.kind, "composition");
    assert_eq!(
        absent.outcome.resolution,
        processing::protocol::ResolutionState::NoResult
    );

    let singular = process_expression_with_engine(
        request("Lagrange(x+y,(x^2+y^2)^2,x,y)", false),
        &mut engine,
    )
    .unwrap();
    assert!(matches!(
        singular.analysis.as_ref(),
        Some(processing::semantic_core::ComputationAnalysis::Lagrange(result))
            if result.status == processing::extrema::LagrangeStatus::SingularConstraint
    ));
    assert_eq!(
        singular.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(singular.expression.starts_with("Lagrange("));

    let rejected_outer =
        process_expression_with_engine(request("D(x)Extrema(x^2+y^2,x,y)", false), &mut engine)
            .unwrap();
    assert_eq!(
        rejected_outer.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(rejected_outer.expression.starts_with("D(x)"));
}
