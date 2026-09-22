use super::*;
use processing::binding::SymbolRole;
use std::collections::BTreeSet;

fn request(expression: &str, steps: bool) -> ProcessExpressionRequest {
    ProcessExpressionRequest {
        expression: expression.into(),
        steps,
        verbosity: "standard".into(),
        assumptions: Vec::new(),
    }
}

#[test]
fn one_shot_assumptions_are_restored_after_success_and_failure() {
    let mut engine = RustEngineProxy::spawn().unwrap();
    processing::assumptions::assume(
        &mut engine,
        "y",
        processing::assumptions::AssumptionFact::Real,
    )
    .unwrap();
    let session_assumptions = processing::assumptions::list_assumptions(&mut engine).unwrap();
    let mut scoped = request("Simplify(Sqrt(x^2))", false);
    scoped
        .assumptions
        .push(crate::expression_protocol::ProcessExpressionAssumption {
            symbol: "x".into(),
            fact: processing::assumptions::AssumptionFact::Positive,
        });
    assert_eq!(
        process_expression_with_engine(scoped, &mut engine)
            .unwrap()
            .expression,
        "x"
    );
    assert_eq!(
        processing::assumptions::list_assumptions(&mut engine).unwrap(),
        session_assumptions
    );

    let mut invalid = request("D(x", false);
    invalid
        .assumptions
        .push(crate::expression_protocol::ProcessExpressionAssumption {
            symbol: "x".into(),
            fact: processing::assumptions::AssumptionFact::Positive,
        });
    assert!(process_expression_with_engine(invalid, &mut engine).is_err());
    assert_eq!(
        processing::assumptions::list_assumptions(&mut engine).unwrap(),
        session_assumptions
    );

    let mut invalid_assumption = request("x", false);
    invalid_assumption
        .assumptions
        .push(crate::expression_protocol::ProcessExpressionAssumption {
            symbol: "x;Echo(1)".into(),
            fact: processing::assumptions::AssumptionFact::Positive,
        });
    assert!(process_expression_with_engine(invalid_assumption, &mut engine).is_err());
    assert_eq!(
        processing::assumptions::list_assumptions(&mut engine).unwrap(),
        session_assumptions
    );
}

mod calculus_equations;
mod composition;
mod domain_products;
mod geometry;
mod gui_contract;
mod product_core;
