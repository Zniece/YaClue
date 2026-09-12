use processing::derivatives::{
    derivative_computation_with_user_catalog, FunctionParameterRole, UserFunctionDerivativeCatalog,
    UserFunctionDerivativeRule, UserFunctionPartialDerivative,
};
use processing::engine::RustEngine;
use processing::protocol::Condition;

#[test]
fn request_scoped_user_rules_are_composable_and_do_not_mutate_default_behavior() {
    let catalog = UserFunctionDerivativeCatalog::new(vec![
        UserFunctionDerivativeRule {
            id: "user.square-law.derivative".into(),
            head: "SquareLaw".into(),
            parameters: vec!["u".into()],
            parameter_roles: vec![FunctionParameterRole::Argument],
            partial_derivatives: vec![Some(UserFunctionPartialDerivative {
                expression: "2*u".into(),
                conditions: vec![Condition::Real {
                    expression: "u".into(),
                }],
            })],
        },
        UserFunctionDerivativeRule {
            id: "user.triple.derivative".into(),
            head: "Triple".into(),
            parameters: vec!["u".into()],
            parameter_roles: vec![FunctionParameterRole::Argument],
            partial_derivatives: vec![Some(UserFunctionPartialDerivative {
                expression: "3".into(),
                conditions: Vec::new(),
            })],
        },
    ])
    .unwrap();
    let mut engine = RustEngine::spawn().unwrap();

    let registered = derivative_computation_with_user_catalog(
        &mut engine,
        "SquareLaw(Sin(x))",
        "x",
        1,
        &catalog,
    )
    .unwrap();
    let value = registered.value().unwrap();
    assert_eq!(value.print_source(), "2*Sin(x)*Cos(x)");
    assert!(value.semantics.metadata.conditions.conditions().iter().any(
        |condition| matches!(condition, Condition::Real { expression } if expression == "Sin(x)")
    ));

    let nested = derivative_computation_with_user_catalog(
        &mut engine,
        "SquareLaw(Triple(x))",
        "x",
        1,
        &catalog,
    )
    .unwrap();
    let nested_source = nested.value().unwrap().print_source();
    assert!(nested_source.contains("2*Triple(x)"), "{nested_source}");
    assert!(nested_source.contains("*3"), "{nested_source}");

    let unregistered =
        processing::derivatives::derivative_computation(&mut engine, "SquareLaw(Sin(x))", "x", 1)
            .unwrap();
    assert!(unregistered.value().is_none());
    assert!(unregistered
        .subject()
        .unwrap()
        .print_source()
        .starts_with("D(x,1)"));
}
