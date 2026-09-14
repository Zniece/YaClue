//! Compile-time-adjacent guards for the semantic execution dependency graph.
//!
//! These inspect the checked-in sources rather than relying on conventions
//! that can silently drift. They intentionally cover architectural modules,
//! not compatibility tests or product projection code.

const VISITOR: &str = include_str!("../src/execution_visitor.rs");
const HANDLERS: &str = include_str!("../src/operator_handlers.rs");
const ARITHMETIC: &str = include_str!("../src/arithmetic.rs");
const COMPOSITION_EXECUTION: &str = include_str!("../src/composition/execution.rs");
const DERIVATIVES: &str = include_str!("../src/derivatives.rs");
const LIMITS: &str = include_str!("../src/limits.rs");
const INTEGRALS: &str = include_str!("../src/integrals.rs");
const SERIES: &str = include_str!("../src/series.rs");
const MULTIPLE_INTEGRALS: &str = include_str!("../src/multiple_integrals.rs");
const EQUATIONS: &str = include_str!("../src/equations.rs");
const ODE: &str = include_str!("../src/ode.rs");
const LINEAR_ALGEBRA: &str = include_str!("../src/linear_algebra.rs");
const EXTREMA: &str = include_str!("../src/extrema.rs");
const LINE_INTEGRALS: &str = include_str!("../src/line_integrals.rs");
const SURFACE_INTEGRALS: &str = include_str!("../src/surface_integrals.rs");
const IMPROPER_INTEGRALS: &str = include_str!("../src/improper_integrals.rs");
const OBJECTS: &str = include_str!("../src/objects.rs");
const INTRINSICS: &str = include_str!("../src/intrinsics.rs");
const EQUIVALENCE: &str = include_str!("../src/equivalence.rs");
const PROCESSING_LIB: &str = include_str!("../src/lib.rs");
const INPUT: &str = include_str!("../src/input.rs");
const APP_EXPRESSION: &str = include_str!("../../app/src-tauri/src/expression.rs");
const STEPS: &str = include_str!("../src/steps.rs");
const TRACE_COMPUTATION: &str = include_str!("../src/semantic_core/trace_computation.rs");

fn production(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn assert_excludes(source: &str, owner: &str, forbidden: &[&str]) {
    for dependency in forbidden {
        assert!(
            !source.contains(dependency),
            "{owner} must not depend on {dependency}"
        );
    }
}

#[test]
fn semantic_execution_layers_do_not_depend_on_product_projection_or_app() {
    for (owner, source) in [
        ("execution_visitor", VISITOR),
        ("operator_handlers", HANDLERS),
        ("arithmetic", production(ARITHMETIC)),
        ("composition execution", COMPOSITION_EXECUTION),
    ] {
        assert_excludes(
            source,
            owner,
            &["crate::steps", "crate::composition", "app::", "tauri::"],
        );
    }
}

#[test]
fn domain_execution_does_not_call_back_into_the_recursive_visitor() {
    assert_excludes(
        production(ARITHMETIC),
        "arithmetic",
        &["execution_visitor", "execute_elaborated_structure"],
    );
    assert_excludes(HANDLERS, "operator_handlers", &["crate::execution_visitor"]);
}

#[test]
fn mathematical_domains_do_not_depend_on_step_projection() {
    for (owner, source) in [
        ("derivatives", DERIVATIVES),
        ("limits", LIMITS),
        ("integrals", INTEGRALS),
        ("series", SERIES),
        ("multiple_integrals", MULTIPLE_INTEGRALS),
        ("equations", EQUATIONS),
        ("ode", ODE),
        ("linear_algebra", LINEAR_ALGEBRA),
        ("extrema", EXTREMA),
        ("line_integrals", LINE_INTEGRALS),
        ("surface_integrals", SURFACE_INTEGRALS),
        ("improper_integrals", IMPROPER_INTEGRALS),
        ("objects", OBJECTS),
        ("intrinsics", INTRINSICS),
        ("equivalence", EQUIVALENCE),
    ] {
        assert_excludes(
            production(source),
            owner,
            &["crate::steps", "StepVerbosity", "Vec<Step>"],
        );
    }
}

#[test]
fn composition_execution_has_one_semantic_consumer_and_no_product_types() {
    assert!(COMPOSITION_EXECUTION.contains("crate::execution_visitor"));
    assert_excludes(
        COMPOSITION_EXECUTION,
        "composition execution",
        &["Step", "CompositionResult", "MathematicalConclusion"],
    );
}

#[test]
fn product_client_does_not_reconstruct_processing_authority() {
    assert!(APP_EXPRESSION.contains("composition::classify_product"));
    assert!(APP_EXPRESSION.contains("composition::project_partial"));
    assert_excludes(
        APP_EXPRESSION,
        "app expression adapter",
        &[
            "MathematicalForm",
            "operator_descriptor",
            "has_object_native_descendant",
            "binder_scopes",
            "symbol_identities.iter_mut",
        ],
    );
}

#[test]
fn removed_and_legacy_facades_cannot_reenter_the_default_product() {
    assert_excludes(
        INPUT,
        "input boundary",
        &["RootCall", "root_call(", "root_call_from_tree"],
    );
    assert!(PROCESSING_LIB.contains(
        "#[cfg(any(test, feature = \"legacy-step-api\"))]\n#[doc(hidden)]\npub mod step_compatibility;"
    ));
    assert_excludes(
        production(STEPS),
        "step product projection",
        &[
            "pub use crate::step_compatibility",
            "pub use crate::integrals",
        ],
    );
}

#[test]
fn derivative_rule_revisions_consume_structured_ast_snapshots() {
    let start = DERIVATIVES.find("struct DerivativeRuleEmission").unwrap();
    let end = DERIVATIVES
        .find("struct FunctionPartialDerivative")
        .unwrap();
    let rule_pipeline = &DERIVATIVES[start..end];
    assert!(rule_pipeline.contains("expression_ast:"));
    assert!(rule_pipeline.contains("to_canonical_ast"));
    assert!(rule_pipeline.contains("materialize_rule_transitions_from_ast"));
    assert_excludes(
        rule_pipeline,
        "derivative rule transition pipeline",
        &["parse_engine_expression"],
    );
}

#[test]
fn transition_materializers_name_their_representation_boundary() {
    assert!(TRACE_COMPUTATION.contains("materialize_rule_transitions_from_ast"));
    assert!(TRACE_COMPUTATION.contains("materialize_rule_transitions_from_engine_source"));
    assert!(!TRACE_COMPUTATION.contains("fn materialize_rule_transitions("));
}
