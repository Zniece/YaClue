use super::*;

#[test]
fn unified_input_preserves_unlowered_structured_compositions() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    {
        let expression = "Factor(DoubleIntegral(f(x,y),y,0,x,x,0,1))";
        let result =
            process_expression_with_engine(request(expression, true), &mut engine).unwrap();
        assert_eq!(result.kind, "composition", "{expression}");
        assert_eq!(result.expression, expression);
        assert_eq!(
            result.outcome.support,
            processing::protocol::SupportState::Supported,
            "{expression}"
        );
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Unresolved,
            "{expression}"
        );
        assert!(!result.steps.is_empty());
        assert_eq!(result.semantic.kind, ValueKind::Unevaluated);
    }

    let matrix_solution = process_expression_with_engine(
        request("N(MatrixSolve({{1,0},{0,1}},{1,2}),10)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(matrix_solution.expression, "N({1,2},10)");
    assert_eq!(
        matrix_solution.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(matrix_solution
        .steps
        .iter()
        .any(|step| step.rule == "matrix-solve"));

    let solved_set =
        process_expression_with_engine(request("D(x)Solve({x==1},{x})", true), &mut engine)
            .unwrap();
    assert_eq!(solved_set.kind, "composition");
    assert_eq!(solved_set.expression, "D(x){x==1}");
    assert_eq!(
        solved_set.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(solved_set
        .steps
        .iter()
        .any(|step| step.rule == "solve-equations"));
}

#[test]
fn unified_input_composes_functions_with_semantic_calculus_results() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let result =
        process_expression_with_engine(request("Sin(Limit(t,0)(Sin(t)/t))", false), &mut engine)
            .unwrap();
    assert_eq!(result.kind, "composition");
    assert_eq!(result.expression, "Sin(1)");
    assert_eq!(result.semantic.kind, ValueKind::Scalar);

    let collection = process_expression_with_engine(
        request("{Limit(t,0)(Sin(t)/t),D(x)(x^2)}", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(collection.expression, "{1,2*x}");

    let relation =
        process_expression_with_engine(request("(Limit(t,0)(Sin(t)/t))==1", false), &mut engine)
            .unwrap();
    assert_eq!(relation.expression, "1==1");
    assert_eq!(relation.semantic.kind, ValueKind::Equation);
}

#[test]
fn unified_input_exposes_registered_special_function_derivatives() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for expression in [
        "D(x)Erf(x^2)",
        "D(x)PolyGamma(2,Sin(x))",
        "D(x)LambertW(Exp(x))",
        "D(x)Beta(x,x^2)",
        "D(x)IncompleteGamma(x^2,x+1)",
        "D(x)BesselJ(n,Sin(x^2))",
    ] {
        let result =
            process_expression_with_engine(request(expression, true), &mut engine).unwrap();
        assert_eq!(result.kind, "derivative", "{expression}");
        assert!(!result.expression.contains("D("), "{expression}");
        assert_eq!(result.semantic.kind, ValueKind::Expression, "{expression}");
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "derivative-registered-function-chain-rule"));
    }

    let incomplete =
        process_expression_with_engine(request("D(x)IncompleteGamma(x^2,x+1)", true), &mut engine)
            .unwrap();
    assert!(incomplete.expression.contains("Integrate("));
    assert!(incomplete.expression.contains("Ln("));
    assert!(incomplete
            .outcome
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, processing::protocol::Condition::RealPartPositive { expression } if expression == "x+1")));

    let held =
        process_expression_with_engine(request("D(x)PolyGamma(x,x)", true), &mut engine).unwrap();
    assert_eq!(held.kind, "derivative");
    assert!(held.expression.contains("D(x,1)"));
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );

    let varying_order =
        process_expression_with_engine(request("D(x)BesselJ(x,x^2)", true), &mut engine).unwrap();
    assert!(varying_order.expression.contains("D(x,1)"));
    assert!(varying_order.expression.contains("BesselJ(x,x^2)"));
    assert_eq!(
        varying_order.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );

    let without_trace =
        process_expression_with_engine(request("D(x)Beta(Sin(x),x^2)", false), &mut engine)
            .unwrap();
    assert!(without_trace.steps.is_empty());
    assert!(!without_trace.expression.contains("D("));
    assert_eq!(
        without_trace.outcome.resolution,
        processing::protocol::ResolutionState::Solved
    );
}

#[test]
fn unified_input_preserves_known_formal_special_function_derivatives() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for expression in [
        "D(x)Zeta(Sin(x))",
        "D(x)EllipticK(x^2)",
        "D(x)EllipticE(x)",
        "D(x)HypergeometricPFQ({a,b},{c},x)",
    ] {
        let result =
            process_expression_with_engine(request(expression, true), &mut engine).unwrap();
        assert_eq!(result.kind, "derivative", "{expression}");
        assert!(result.expression.starts_with("D(x,1)"), "{expression}");
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Unresolved,
            "{expression}"
        );
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "derivative-known-formal-function"));
    }
}

#[test]
fn unified_input_exposes_typed_partials_and_reclassifies_final_symbols() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    for (expression, operator) in [
        ("D(x)", processing::semantic_core::OperatorId::Derivative),
        (
            "Integrate(x)",
            processing::semantic_core::OperatorId::Integral,
        ),
    ] {
        let result =
            process_expression_with_engine(request(expression, false), &mut engine).unwrap();
        assert_eq!(result.kind, "partial_application", "{expression}");
        assert_eq!(result.expression, expression);
        assert_eq!(result.semantic.kind, ValueKind::Unevaluated);
        assert_eq!(result.semantic.bound_symbols, ["x"]);
        let Some(ProcessExpressionDetails::PartialApplication { partial }) =
            result.details.as_ref()
        else {
            panic!("expected typed partial application")
        };
        assert_eq!(partial.operator, operator);
        assert_eq!(
            partial.missing,
            [processing::semantic_core::Requirement::Operand]
        );
        assert_eq!(
            result.outcome.resolution,
            processing::protocol::ResolutionState::Unresolved
        );
    }

    let overloaded =
        process_expression_with_engine(request("Limit(t)", false), &mut engine).unwrap();
    assert_eq!(overloaded.kind, "partial_application");
    let Some(ProcessExpressionDetails::AmbiguousPartialApplication {
        candidates,
        display_templates,
    }) = overloaded.details.as_ref()
    else {
        panic!("expected typed ambiguous partial application")
    };
    assert_eq!(candidates.len(), 3);
    assert!(display_templates
        .iter()
        .any(|template| template == "Limit(t)(<approach_point>)(<operand>)"));

    let completed = process_expression_with_engine(
        request("Limit(t,0)(D(x)(Integrate(x)(Sin(t)/t+x^2)))", false),
        &mut engine,
    )
    .unwrap();
    assert_eq!(completed.expression, "x^2+1");
    assert_eq!(completed.semantic.symbols, ["x"]);
    assert!(completed.semantic.bound_symbols.is_empty());
    assert!(completed.semantic.symbol_identities.iter().any(|identity| {
        identity.name == "x"
            && identity.role == processing::binding::SymbolRole::Free
            && identity.binder.is_none()
    }));
}

#[test]
fn unified_input_routes_nested_algebra_and_preserves_held_structure() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let nested =
        process_expression_with_engine(request("Sin(Factor(x^2-1))", true), &mut engine).unwrap();
    assert_eq!(nested.kind, "composition");
    assert_eq!(nested.expression, "Sin((x+1)*(x-1))");
    assert!(nested
        .steps
        .iter()
        .any(|step| step.rule == "apply-algebra-transform"));

    let held =
        process_expression_with_engine(request("Factor(Integrate(x)f(x))", false), &mut engine)
            .unwrap();
    assert_eq!(held.expression, "Factor(Integrate(x)f(x))");
    assert_eq!(held.semantic.kind, ValueKind::Unevaluated);
    assert_eq!(held.semantic.bound_symbols, ["x"]);
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(held
        .operators
        .contains(&processing::semantic_core::OperatorId::Factor));
}

#[test]
fn unified_input_routes_object_native_substitution() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let direct =
        process_expression_with_engine(request("Subst(x,2)(x^2+1)", true), &mut engine).unwrap();
    assert_eq!(direct.kind, "composition");
    assert_eq!(direct.expression, "5");
    assert_eq!(direct.semantic.kind, ValueKind::Scalar);
    assert!(direct.semantic.symbols.is_empty());
    assert!(direct
        .steps
        .iter()
        .any(|step| step.rule == "substitute-free-symbol"));

    let held = process_expression_with_engine(
        request("Subst(x,2)((Integrate(t)f(t))+x)", true),
        &mut engine,
    )
    .unwrap();
    assert_eq!(held.kind, "composition");
    assert_eq!(held.semantic.kind, ValueKind::Unevaluated);
    assert!(held.expression.contains("Integrate"));
    assert!(held.expression.contains("+2"));
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
}

#[test]
fn unified_input_routes_object_native_numeric_evaluation() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let value = process_expression_with_engine(request("N(Pi,20)", true), &mut engine).unwrap();
    assert_eq!(value.kind, "composition");
    assert_eq!(value.semantic.kind, ValueKind::Scalar);
    assert_eq!(
        value.semantic.exactness,
        processing::semantic::Exactness::Approximate
    );
    assert!(value
        .steps
        .iter()
        .any(|step| step.rule == "numeric-evaluation"));

    let held =
        process_expression_with_engine(request("N(D(x)(x^2),20)", false), &mut engine).unwrap();
    assert_eq!(held.expression, "N(2*x,20)");
    assert_eq!(held.semantic.kind, ValueKind::Unevaluated);
    assert_eq!(held.semantic.symbols, ["x"]);
    assert!(held.semantic.bound_symbols.is_empty());
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );

    let absent =
        process_expression_with_engine(request("N(Undefined,20)", false), &mut engine).unwrap();
    assert_eq!(
        absent.outcome.resolution,
        processing::protocol::ResolutionState::NoResult
    );
}

#[test]
fn unified_input_routes_object_native_sums_and_compositions() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    let direct =
        process_expression_with_engine(request("Sum(k,1,10,k)", true), &mut engine).unwrap();
    assert_eq!(direct.kind, "series");
    assert_eq!(direct.expression, "55");
    assert_eq!(direct.semantic.kind, ValueKind::Scalar);
    assert!(direct.semantic.symbols.is_empty());
    assert!(direct.semantic.bound_symbols.is_empty());
    assert!(direct.steps.iter().any(|step| step.rule == "finite-sum"));

    let nested =
        process_expression_with_engine(request("D(x)Sum(k,1,3,x*k)", true), &mut engine).unwrap();
    assert_eq!(nested.kind, "composition");
    assert_eq!(nested.expression, "6");
    assert!(nested.semantic.symbols.is_empty());
    assert!(nested.steps.iter().any(|step| step.rule == "finite-sum"));

    let divergent =
        process_expression_with_engine(request("Sum(k,1,Infinity,1/k)", true), &mut engine)
            .unwrap();
    assert_eq!(
        divergent.outcome.resolution,
        processing::protocol::ResolutionState::NoResult
    );
    assert_eq!(
        divergent.outcome.reason,
        Some(processing::protocol::OutcomeReason::Divergent)
    );

    let held =
        process_expression_with_engine(request("Sum(k,0,Infinity,1/k^2)", false), &mut engine)
            .unwrap();
    assert_eq!(held.semantic.kind, ValueKind::Unevaluated);
    assert_eq!(
        held.outcome.resolution,
        processing::protocol::ResolutionState::Unresolved
    );
    assert!(held.expression.starts_with("Sum("));
}
