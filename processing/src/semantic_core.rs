//! The semantic-core boundary between the Yacas AST and product domains.
//!
//! This module deliberately starts small.  It does not introduce a second
//! expression tree and it does not move any domain algorithm yet.  A
//! `MathematicalObject` owns the current Yacas expression and carries a sparse
//! semantic overlay.  Domains will gradually return `Transition`s and
//! `RuleEvent`s through this boundary.

mod cache_session;
mod normalization_representation;
mod object_state;
mod operator_signature;
mod trace_computation;

pub(crate) use cache_session::CachedOperationResult;
pub use cache_session::{OperationCacheBudget, OperationCacheKey, OperationSessionAst};
pub use normalization_representation::{
    NormalizationLevel, NormalizationMetadata, NormalizationMode, NormalizationState,
    RepresentationId, RepresentationPreference,
};
pub(crate) use object_state::parse_engine_expression;
pub use object_state::*;
pub use operator_signature::{
    complete_operand_partial, fill_next_partial_argument, is_known_operator,
    is_object_native_operator, object_native_route, operand_partial_state, operator_descriptor,
    partial_application_state, promote_held_application, promote_registered_held_expression,
    require_operand_partial, typed_application_state, ApplicationForm, ApplicationSlot,
    BinderDescriptor, BinderScope, BoundArgument, CapabilityId, ObjectNativeRoute,
    OperatorDescriptor, OperatorExecutionHandler, OperatorId, OperatorSlotSignature,
    PartialApplication, RecursiveExecutionHandler, ResultTypeConstraint, ScopeArgument,
    TypedApplication, TypedArgument, ValueArgument, OPERATOR_DESCRIPTORS,
};
pub use trace_computation::*;
pub(crate) use trace_computation::{
    materialize_rule_transitions_from_ast, materialize_rule_transitions_from_engine_source,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeSet;
    use yacas_rs::env::Environment;
    use yacas_rs::parser::parse_expression;

    use crate::protocol::ResultMetadata;
    use crate::semantic::ValueKind;

    use super::normalization_representation::MAX_STABLE_REPRESENTATIONS;

    #[test]
    fn operator_registry_is_internally_complete_and_unambiguous() {
        let mut names = BTreeSet::new();
        for descriptor in OPERATOR_DESCRIPTORS {
            assert!(!descriptor.names.is_empty());
            for name in descriptor.names {
                assert!(names.insert(*name), "duplicate operator spelling: {name}");
                assert_eq!(operator_descriptor(name).unwrap().id, descriptor.id);
            }

            let declared_arities: BTreeSet<_> = descriptor.arities.iter().copied().collect();
            let signature_arities: BTreeSet<_> = descriptor
                .slot_signatures
                .iter()
                .map(|signature| {
                    assert_eq!(signature.arity, signature.requirements.len());
                    signature.arity
                })
                .collect();
            assert_eq!(
                declared_arities, signature_arities,
                "{:?}",
                descriptor.names
            );

            for arity in descriptor.arities {
                for binder in descriptor.binders {
                    assert!(binder.binder_argument < *arity, "{:?}", descriptor.names);
                    let scope = match binder.scope_argument {
                        ScopeArgument::First => 0,
                        ScopeArgument::Last => arity - 1,
                        ScopeArgument::Index(index) => index,
                    };
                    assert!(scope < *arity, "{:?}", descriptor.names);
                    // A descriptor may support a shorter non-binding arity
                    // (for example Limit(point, body)); binding activates only
                    // where this slot is explicitly typed as a variable.
                }
            }

            for name in descriptor.names {
                assert_eq!(object_native_route(name), Some(descriptor.route));
            }
        }

        let limit = operator_descriptor("Limit").unwrap();
        assert_eq!(limit.operand_index(2), Some(0));
        assert_eq!(limit.operand_index(3), Some(2));
        assert_eq!(limit.operand_index(4), Some(3));
        let taylor = operator_descriptor("Taylor").unwrap();
        assert_eq!(taylor.operand_index(3), Some(0));
        assert_eq!(taylor.operand_index(4), Some(3));

        assert_eq!(
            operator_descriptor("Plot")
                .unwrap()
                .product_presentation(true, true),
            Some("plot")
        );
        assert_eq!(
            operator_descriptor("D")
                .unwrap()
                .product_presentation(true, true),
            None
        );
        assert_eq!(
            operator_descriptor("Factor")
                .unwrap()
                .product_presentation(false, false),
            None
        );
    }

    #[test]
    fn every_descriptor_has_a_type_checked_execution_strategy() {
        assert!(!OPERATOR_DESCRIPTORS.is_empty());
        for descriptor in OPERATOR_DESCRIPTORS {
            let _: OperatorExecutionHandler = descriptor.execution_handler;
            assert!(
                !descriptor.names.is_empty(),
                "execution strategy has no surface spelling: {:?}",
                descriptor.id
            );
        }
    }

    #[test]
    fn view_borrows_one_ast_without_reparsing_children() {
        let mut env = Environment::new();
        let tree = parse_expression(&mut env, "Taylor(Exp(x),0,2);")
            .unwrap()
            .unwrap();
        let view = ExpressionView::new(&env, &tree);
        assert_eq!(view.kind(), NodeKind::Application);
        assert_eq!(view.head(), Some("Taylor"));
        let args = view.arguments();
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].head(), Some("Exp"));
        assert_eq!(args[0].arguments()[0].atom(), Some("x"));
        assert_eq!(args[1].number(), Some("0".into()));
    }

    #[test]
    fn transition_updates_ast_and_semantics_as_one_delta() {
        let mut env = Environment::new();
        let old = parse_expression(&mut env, "x;").unwrap().unwrap();
        let new = parse_expression(&mut env, "x+1;").unwrap().unwrap();
        let metadata = ResultMetadata::unresolved(
            crate::semantic::Exactness::Symbolic,
            crate::protocol::OutcomeReason::AlgorithmUncovered,
        );
        let mut object = MathematicalObject::new(
            ObjectId(1),
            old,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: metadata.clone(),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        object.apply(ObjectDelta {
            expression: Some(new),
            semantics: Some(SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::StructuredUnevaluated {
                    reason: "test".into(),
                },
                metadata,
                capabilities: CapabilitySet::empty(),
                requirements: Vec::new(),
            }),
            overlay: None,
            normalization: None,
        });
        assert_eq!(object.view(&env).print_source(), "x+1");
        assert_eq!(object.semantics.kind, ValueKind::Unevaluated);
        assert_eq!(object.revision, ObjectRevision(1));
    }

    #[test]
    fn rule_references_keep_the_ast_revision_that_was_observed() {
        let mut env = Environment::new();
        let initial = parse_expression(&mut env, "x;").unwrap().unwrap();
        let replacement = parse_expression(&mut env, "1;").unwrap().unwrap();
        let metadata = ResultMetadata::solved(
            crate::semantic::Exactness::Exact,
            crate::protocol::ConditionSet::empty(),
        );
        let mut object = MathematicalObject::new(
            ObjectId(7),
            initial,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: metadata.clone(),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        let before = object.reference(Some(ExpressionPath::root()));
        object.apply(ObjectDelta {
            expression: Some(replacement),
            semantics: Some(SemanticState {
                kind: ValueKind::Scalar,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            }),
            overlay: None,
            normalization: None,
        });
        let after = object.reference(Some(ExpressionPath::root()));
        assert_eq!(before.object, after.object);
        assert_eq!(before.revision, ObjectRevision(0));
        assert_eq!(after.revision, ObjectRevision(1));
    }

    #[test]
    fn equivalent_representations_preserve_identity_and_version_ast_snapshots() {
        let mut env = Environment::new();
        let expanded = parse_expression(&mut env, "x^2-1;").unwrap().unwrap();
        let factored = parse_expression(&mut env, "(x-1)*(x+1);").unwrap().unwrap();
        let metadata = ResultMetadata::solved(
            crate::semantic::Exactness::Exact,
            crate::protocol::ConditionSet::empty(),
        );
        let mut object = MathematicalObject::new(
            ObjectId(30),
            expanded,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        let identity = object.identity();
        let alternative = object
            .retain_representation(
                factored,
                RepresentationPreference::Named("factored".into()),
                None,
            )
            .unwrap();

        assert_eq!(object.stable_representation_count(), 2);
        assert_eq!(object.revision, ObjectRevision(0));
        object.activate_representation(alternative).unwrap();
        assert_eq!(object.identity(), identity);
        assert_eq!(object.revision, ObjectRevision(1));
        assert_eq!(object.view(&env).print_source(), "(x-1)*(x+1)");
        assert_eq!(
            object.representation_preference(alternative),
            Some(&RepresentationPreference::Named("factored".into()))
        );

        object.activate_representation(RepresentationId(0)).unwrap();
        assert_eq!(object.identity(), identity);
        assert_eq!(object.revision, ObjectRevision(2));
        assert_eq!(object.view(&env).print_source(), "x^2-1");
    }

    #[test]
    fn operation_session_ast_is_temporary_and_stable_candidates_are_bounded() {
        let mut env = Environment::new();
        let principal = parse_expression(&mut env, "Gamma(x);").unwrap().unwrap();
        let metadata = ResultMetadata::solved(
            crate::semantic::Exactness::Exact,
            crate::protocol::ConditionSet::empty(),
        );
        let mut object = MathematicalObject::new(
            ObjectId(31),
            principal,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        let original_revision = object.revision;
        let mut newest = RepresentationId(0);
        for index in 0..5 {
            let expression = parse_expression(&mut env, &format!("Gamma(x)+{index};"))
                .unwrap()
                .unwrap();
            newest = object
                .retain_representation(
                    expression,
                    RepresentationPreference::Operation(OperatorId::Derivative),
                    None,
                )
                .unwrap();
        }
        assert_eq!(
            object.stable_representation_count(),
            MAX_STABLE_REPRESENTATIONS
        );

        let session = object.operation_session(Some(newest)).unwrap();
        assert_eq!(session.identity, object.identity());
        assert_eq!(session.revision, original_revision);
        assert_eq!(session.view(&env).print_source(), "Gamma(x)+4");
        assert_eq!(object.view(&env).print_source(), "Gamma(x)");
        assert_eq!(object.revision, original_revision);

        object.activate_representation(newest).unwrap();
        let active_source = object.print_source();
        let extra = parse_expression(&mut env, "Gamma(x)+99;").unwrap().unwrap();
        object
            .retain_representation(
                extra,
                RepresentationPreference::Operation(OperatorId::Derivative),
                None,
            )
            .unwrap();
        assert_eq!(object.print_source(), active_source);
        assert!(object.operation_session(Some(newest)).is_ok());
        assert_eq!(
            object.stable_representation_count(),
            MAX_STABLE_REPRESENTATIONS
        );
    }

    #[test]
    fn operation_cache_keys_every_semantic_context_and_enforces_budgets() {
        let mut env = Environment::new();
        let expression = parse_expression(&mut env, "x+1;").unwrap().unwrap();
        let metadata = ResultMetadata::solved(
            crate::semantic::Exactness::Exact,
            crate::protocol::ConditionSet::empty(),
        );
        let object = MathematicalObject::new(
            ObjectId(33),
            expression.clone(),
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        );
        let key = object.operation_cache_key("Simplify", vec!["x:Positive".into()], Some(20));
        assert_ne!(
            key,
            object.operation_cache_key("Factor", vec!["x:Positive".into()], Some(20))
        );
        assert_ne!(
            key,
            object.operation_cache_key("Simplify", vec!["x:Real".into()], Some(20))
        );
        assert_ne!(
            key,
            object.operation_cache_key("Simplify", vec!["x:Positive".into()], Some(50))
        );
        let mut later = key.clone();
        later.revision = ObjectRevision(1);
        assert_ne!(key, later);
        later = key.clone();
        later.representation = RepresentationId(1);
        assert_ne!(key, later);

        let budget = OperationCacheBudget {
            max_entries: 2,
            max_total_bytes: 64,
            max_entry_bytes: 32,
        };
        let result = |source: &str| CachedOperationResult {
            expression: expression.clone(),
            source: source.into(),
            tex: source.into(),
        };
        assert!(object.cache_operation(key.clone(), result("x+1"), budget));
        let second = object.operation_cache_key("Expand", Vec::new(), None);
        assert!(object.cache_operation(second.clone(), result("x+1"), budget));
        let third = object.operation_cache_key("Factor", Vec::new(), None);
        assert!(object.cache_operation(third.clone(), result("x+1"), budget));
        assert_eq!(object.cached_operation_count(), 2);
        assert!(object.cached_operation(&key).is_none());
        assert!(object.cached_operation(&second).is_some());
        assert!(object.cached_operation(&third).is_some());

        assert!(!object.cache_operation(
            object.operation_cache_key("oversized", Vec::new(), None),
            result("an-expression-that-exceeds-the-entry-budget"),
            budget,
        ));
        assert_eq!(object.cached_operation_count(), 2);
    }

    #[test]
    fn partial_state_records_slots_requirements_and_binder_scope() {
        let partial = operand_partial_state("Limit", 3).unwrap();
        assert_eq!(partial.operator, OperatorId::Limit);
        assert_eq!(partial.expected_arity, 4);
        assert_eq!(partial.missing, vec![Requirement::Operand]);
        assert_eq!(partial.bound_arguments.len(), 3);
        assert_eq!(partial.bound_arguments[2].path.segments(), &[2]);
        assert!(matches!(
            partial.slots.as_slice(),
            [
                ApplicationSlot::Bound { slot: 0, .. },
                ApplicationSlot::Bound { slot: 1, .. },
                ApplicationSlot::Bound { slot: 2, .. },
                ApplicationSlot::Missing {
                    slot: 3,
                    requirement: Requirement::Operand
                }
            ]
        ));
        assert_eq!(
            partial.binder_scopes,
            vec![BinderScope {
                binder_slot: 0,
                scope_slot: 3
            }]
        );
    }

    #[test]
    fn partial_state_is_driven_by_the_shared_operator_descriptor() {
        assert!(operand_partial_state("D", 1).is_ok());
        assert!(operand_partial_state("Integrate", 1).is_ok());
        assert!(operand_partial_state("Factor", 0).is_err());
        assert!(operand_partial_state("D", 3).is_err());
    }

    #[test]
    fn completing_a_partial_links_the_operand_ast_without_string_state() {
        let partial = object_from_source(
            ObjectId(11),
            "D(x)",
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::PartialApplication(
                    operand_partial_state("D", 1).unwrap(),
                ),
                metadata: ResultMetadata::unresolved(
                    crate::semantic::Exactness::Symbolic,
                    crate::protocol::OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: vec![Requirement::Operand],
            },
        )
        .unwrap();
        let operand = object_from_source(
            ObjectId(12),
            "x^2",
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::solved(
                    crate::semantic::Exactness::Symbolic,
                    crate::protocol::ConditionSet::empty(),
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let completed = complete_operand_partial(&partial, &operand).unwrap();
        assert_eq!(completed.print_source(), "D(x)x^2");
        assert!(completed.semantics.requirements.is_empty());
        assert!(matches!(completed.semantics.interpretation,
            SemanticInterpretation::TypedApplication(ref application)
                if application.spelling == "D" && application.free_parameters.is_empty()));
    }

    #[test]
    fn fills_a_multi_stage_signature_one_typed_slot_at_a_time() {
        let state = partial_application_state("Limit", 3, 1).unwrap();
        let partial = object_from_source(
            ObjectId(20),
            "Limit(t)",
            SemanticState {
                kind: ValueKind::Unevaluated,
                interpretation: SemanticInterpretation::PartialApplication(state.clone()),
                metadata: ResultMetadata::unresolved(
                    crate::semantic::Exactness::Symbolic,
                    crate::protocol::OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: state.missing,
            },
        )
        .unwrap();
        let scalar = |id, source| {
            object_from_source(
                ObjectId(id),
                source,
                SemanticState {
                    kind: ValueKind::Scalar,
                    interpretation: SemanticInterpretation::PlainExpression,
                    metadata: ResultMetadata::solved(
                        crate::semantic::Exactness::Exact,
                        crate::protocol::ConditionSet::empty(),
                    ),
                    capabilities: CapabilitySet::symbolic_expression(),
                    requirements: Vec::new(),
                },
            )
            .unwrap()
        };
        let with_point = fill_next_partial_argument(&partial, &scalar(21, "0")).unwrap();
        assert_eq!(with_point.print_source(), "Limit(t)0");
        assert_eq!(with_point.semantics.requirements, [Requirement::Operand]);
        assert!(matches!(
            with_point.semantics.interpretation,
            SemanticInterpretation::PartialApplication(_)
        ));
        let complete = fill_next_partial_argument(&with_point, &scalar(22, "x/t")).unwrap();
        assert_eq!(complete.print_source(), "Limit(t,0)x/t");
        assert!(complete.semantics.requirements.is_empty());
        assert!(matches!(
            complete.semantics.interpretation,
            SemanticInterpretation::TypedApplication(_)
        ));
    }

    #[test]
    fn mathematical_step_protocol_separates_visibility_from_trace_payload() {
        assert!(RuleEventClass::EquivalentTransformation.is_transformation_step());
        assert!(!RuleEventClass::MathematicalAnalysis.is_transformation_step());
        assert!(RuleEventClass::MathematicalAnalysis.is_product_visible());
        assert!(RuleEventClass::MathematicalConclusion.is_product_visible());
        assert!(RuleEventClass::ProductEffect.is_product_visible());
        assert!(!RuleEventClass::InternalExecution.is_product_visible());
        assert!(!RuleEventClass::MathematicalConclusion.is_transformation_step());
    }

    #[test]
    fn computation_context_is_request_scoped_and_restored_after_nesting() {
        assert_eq!(
            current_computation_context().trace_mode,
            TraceMode::Detailed
        );
        with_computation_context(ComputationContext::new(TraceMode::Off), || {
            assert_eq!(current_computation_context().trace_mode, TraceMode::Off);
            with_computation_context(ComputationContext::new(TraceMode::Compact), || {
                assert_eq!(current_computation_context().trace_mode, TraceMode::Compact);
            });
            assert_eq!(current_computation_context().trace_mode, TraceMode::Off);
        });
        assert_eq!(
            current_computation_context().trace_mode,
            TraceMode::Detailed
        );
    }

    #[test]
    fn event_sink_keeps_rule_facts_identical_and_materializes_only_presentations() {
        let fact = RuleFact {
            rule: "test-rewrite".into(),
            class: RuleEventClass::EquivalentTransformation,
            input: ObjectReference {
                object: ObjectId(1),
                revision: ObjectRevision(0),
                focus: None,
            },
            additional_inputs: Vec::new(),
            output: ObjectReference {
                object: ObjectId(1),
                revision: ObjectRevision(1),
                focus: None,
            },
            bindings: vec![("variable".into(), "x".into())],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            transformation: None,
        };
        let presentation_builds = Cell::new(0);
        let mut off = VecEventSink::default();
        with_computation_context(ComputationContext::new(TraceMode::Off), || {
            off.record_fact(fact.clone(), || {
                presentation_builds.set(presentation_builds.get() + 1);
                Some(RulePresentation {
                    expression: "1".into(),
                    explanation: "rewrite".into(),
                    tex_override: None,
                })
            });
        });
        let mut detailed = VecEventSink::default();
        with_computation_context(ComputationContext::new(TraceMode::Detailed), || {
            detailed.record_fact(fact, || {
                presentation_builds.set(presentation_builds.get() + 1);
                Some(RulePresentation {
                    expression: "1".into(),
                    explanation: "rewrite".into(),
                    tex_override: None,
                })
            });
        });

        assert_eq!(presentation_builds.get(), 1);
        let off = RuleTrace { events: off.events };
        let detailed = RuleTrace {
            events: detailed.events,
        };
        assert!(off.same_facts_as(&detailed));
        assert!(off.events[0].presentation.is_none());
        assert!(detailed.events[0].presentation.is_some());
    }

    #[test]
    fn transformation_continuity_uses_versioned_object_references() {
        let reference = |revision| ObjectReference {
            object: ObjectId(7),
            revision: ObjectRevision(revision),
            focus: Some(ExpressionPath::root()),
        };
        let event = |input, output| RuleEvent {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: "test".into(),
            input,
            additional_inputs: Vec::new(),
            output,
            bindings: Vec::new(),
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Normal,
            transformation: None,
            presentation: None,
        };
        let continuous = vec![
            event(reference(0), reference(1)),
            event(reference(1), reference(2)),
        ];
        let discontinuous = vec![
            event(reference(0), reference(1)),
            event(reference(0), reference(2)),
        ];
        assert!(transformation_chain_is_continuous(&continuous));
        assert!(!transformation_chain_is_continuous(&discontinuous));
    }

    #[test]
    fn visible_unary_event_has_replayable_ast_context() {
        let event = RuleEvent {
            class: crate::semantic_core::RuleEventClass::EquivalentTransformation,
            rule: "simplify-power".into(),
            input: ObjectReference {
                object: ObjectId(9),
                revision: ObjectRevision(2),
                focus: Some(ExpressionPath::root().argument(1)),
            },
            additional_inputs: Vec::new(),
            output: ObjectReference {
                object: ObjectId(9),
                revision: ObjectRevision(3),
                focus: None,
            },
            bindings: Vec::new(),
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Normal,
            transformation: None,
            presentation: crate::semantic_core::materialize_presentation(|| RulePresentation {
                expression: "4".into(),
                explanation: "计算幂".into(),
                tex_override: None,
            }),
        };
        let context = event.transformation_context().unwrap();
        assert_eq!(context.root_before.focus, None);
        assert_eq!(context.root_after.focus, None);
        assert_eq!(context.focus.segments(), &[1]);

        let mut invalid = event.clone();
        invalid.payload = RulePayload::Decision;
        assert!(invalid.validate_classification().is_err());
        invalid.class = RuleEventClass::MathematicalAnalysis;
        assert!(invalid.validate_classification().is_ok());
    }

    #[test]
    fn certificate_wire_protocol_is_versioned_and_stable() {
        let certificate = Certificate::new(CertificateEvidence::EquivalentRepresentation {
            operation: "factor".into(),
            before: "x^2-1".into(),
            after: "(x-1)*(x+1)".into(),
        });

        assert_eq!(certificate.version(), Certificate::CURRENT_VERSION);
        assert_eq!(
            serde_json::to_value(&certificate).unwrap(),
            serde_json::json!({
                "version": 1,
                "evidence": {
                    "kind": "equivalent_representation",
                    "payload": {
                        "operation": "factor",
                        "before": "x^2-1",
                        "after": "(x-1)*(x+1)"
                    }
                }
            })
        );
    }

    #[test]
    fn typed_analysis_preserves_the_legacy_product_json_shape() {
        let analysis = ComputationAnalysis::MultivariateShape(vec![2]);
        assert_eq!(
            serde_json::to_value(analysis).unwrap(),
            serde_json::json!([2])
        );
    }

    #[test]
    fn extension_certificates_reject_invalid_envelopes() {
        let object = || serde_json::json!({ "proof": "external" });
        assert!(ExtensionCertificateEnvelope::new("", "proof", 1, object()).is_err());
        assert!(ExtensionCertificateEnvelope::new("org.example", "", 1, object()).is_err());
        assert!(ExtensionCertificateEnvelope::new("org.example", "proof", 0, object()).is_err());
        assert!(ExtensionCertificateEnvelope::new(
            "org.example",
            "proof",
            1,
            serde_json::json!("free-form-string")
        )
        .is_err());
        assert!(ExtensionCertificateEnvelope::new("org.example", "proof", 1, object()).is_ok());
    }
}
