//! Parse-once elaboration of every legal product expression into typed AST
//! objects.  This is classification only: domain evaluation remains lazy.

use std::rc::Rc;

use crate::engine::EngineError;
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    CapabilitySet, MathematicalObject, ObjectId, SemanticInterpretation, SemanticState,
};
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathematicalForm {
    Number,
    Symbol,
    Structural { operator: String },
    Relation { operator: String },
    Collection,
    Application { head: String },
    EffectApplication { head: String },
    OpaqueEngineValue { type_name: String },
}

#[derive(Clone)]
pub struct ElaboratedObject {
    pub object: MathematicalObject,
    pub form: MathematicalForm,
    pub children: Vec<ElaboratedObject>,
}

#[derive(Clone)]
pub struct ElaboratedInput {
    pub root: ElaboratedObject,
    pub analyzed: crate::semantic::AnalyzedInput,
}

/// Elaborate once, retaining the engine AST nodes instead of serializing and
/// parsing each child. Every successful product parse has a typed root.
pub fn elaborate(source: &str) -> Result<ElaboratedObject, EngineError> {
    Ok(elaborate_input(source)?.root)
}

/// Single-parse product input boundary: semantic tree and the transitional
/// summary are both derived from the same AST.
pub fn elaborate_input(source: &str) -> Result<ElaboratedInput, EngineError> {
    crate::input::validate_safe_text(source, "语义表达式")?;
    crate::input::with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{source};"))
            .map_err(|error| EngineError::InvalidInput(format!("语义表达式语法错误: {error:?}")))?
            .ok_or_else(|| EngineError::InvalidInput("语义表达式为空".into()))?;
        let mut next_id = 1;
        Ok(ElaboratedInput {
            root: elaborate_node(&tree, &mut next_id),
            analyzed: crate::semantic::analyze_tree(env, &tree),
        })
    })
}

/// Recognize an unambiguous bodied operator whose only missing slot is its
/// operand. Forms that are also valid complete calls (notably two-argument
/// Limit) remain with their established interpretation.
pub fn operand_partial(
    expression: &ElaboratedObject,
) -> Result<Option<MathematicalObject>, EngineError> {
    let MathematicalForm::Application { head } = &expression.form else {
        return Ok(None);
    };
    let Some(descriptor) = crate::semantic_core::operator_descriptor(head) else {
        return Ok(None);
    };
    let bound_count = expression.children.len();
    if descriptor.arities.contains(&bound_count)
        || !descriptor.arities.contains(&(bound_count + 1))
        || descriptor.value_argument != crate::semantic_core::ValueArgument::Last
        || !descriptor
            .forms
            .contains(&crate::semantic_core::ApplicationForm::Bodied)
    {
        return Ok(None);
    }
    for binder in descriptor.binders {
        let Some(argument) = expression.children.get(binder.binder_argument) else {
            return Err(EngineError::InvalidInput(format!(
                "{head} 部分应用缺少绑定变量"
            )));
        };
        let variable = crate::input::with_parse_env(|env| {
            argument
                .object
                .view(env)
                .atom()
                .map(str::to_string)
                .ok_or_else(|| EngineError::InvalidInput(format!("{head} 的绑定变量必须是符号")))
        })?;
        crate::input::validate_symbol(&variable, "绑定变量")?;
    }
    let state = crate::semantic_core::operand_partial_state(head, bound_count)?;
    Ok(Some(MathematicalObject::new(
        expression.object.id,
        expression.object.raw_expression(),
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::PartialApplication(state.clone()),
            metadata: ResultMetadata::unresolved(
                Exactness::Symbolic,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: state.missing,
        },
    )))
}

fn elaborate_node(node: &Rc<LispObject>, next_id: &mut u64) -> ElaboratedObject {
    let (form, children, kind, interpretation, capabilities) = match &node.kind {
        ObjectKind::Number(_) => (
            MathematicalForm::Number,
            Vec::new(),
            ValueKind::Scalar,
            SemanticInterpretation::PlainExpression,
            CapabilitySet::symbolic_expression(),
        ),
        ObjectKind::Atom(_) => (
            MathematicalForm::Symbol,
            Vec::new(),
            ValueKind::Expression,
            SemanticInterpretation::PlainExpression,
            CapabilitySet::symbolic_expression(),
        ),
        ObjectKind::Generic(value) => (
            MathematicalForm::OpaqueEngineValue {
                type_name: value.type_name().into(),
            },
            Vec::new(),
            ValueKind::Unevaluated,
            SemanticInterpretation::StructuredUnevaluated {
                reason: "opaque engine value".into(),
            },
            CapabilitySet::empty(),
        ),
        ObjectKind::Sublist(first) => {
            let nodes: Vec<_> = spine_refs(first).collect();
            let head = nodes
                .first()
                .and_then(|item| item.atom_string())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "Apply".into());
            let children = nodes
                .iter()
                .skip(1)
                .map(|child| elaborate_node(child, next_id))
                .collect();
            if head == "List" {
                if let Some(shape) = crate::semantic::matrix_shape(node) {
                    (
                        MathematicalForm::Collection,
                        children,
                        ValueKind::Matrix,
                        SemanticInterpretation::Matrix {
                            rows: shape.rows,
                            columns: shape.columns,
                        },
                        CapabilitySet::matrix(),
                    )
                } else {
                    (
                        MathematicalForm::Collection,
                        children,
                        ValueKind::Expression,
                        SemanticInterpretation::List,
                        CapabilitySet::equation_input(),
                    )
                }
            } else if matches!(head.as_str(), "+" | "-" | "*" | "/" | "^") {
                (
                    MathematicalForm::Structural {
                        operator: head.clone(),
                    },
                    children,
                    ValueKind::Expression,
                    SemanticInterpretation::PlainExpression,
                    CapabilitySet::symbolic_expression(),
                )
            } else if matches!(head.as_str(), "=" | "==" | "!=" | "<" | ">" | "<=" | ">=") {
                (
                    MathematicalForm::Relation { operator: head },
                    children,
                    ValueKind::Equation,
                    SemanticInterpretation::Equation,
                    CapabilitySet::equation_input(),
                )
            } else if head == "Plot" {
                (
                    MathematicalForm::EffectApplication { head: head.clone() },
                    children,
                    ValueKind::Unevaluated,
                    SemanticInterpretation::Application { operator: head },
                    CapabilitySet::empty(),
                )
            } else {
                (
                    MathematicalForm::Application { head: head.clone() },
                    children,
                    ValueKind::Expression,
                    SemanticInterpretation::Application { operator: head },
                    CapabilitySet::symbolic_expression(),
                )
            }
        }
    };
    let id = ObjectId(*next_id);
    *next_id += 1;
    let immediately_usable = matches!(form, MathematicalForm::Number | MathematicalForm::Symbol)
        || matches!(&form, MathematicalForm::Application { head }
            if !crate::semantic_core::is_known_operator(head))
        || matches!(
            form,
            MathematicalForm::Relation { .. } | MathematicalForm::Collection
        );
    ElaboratedObject {
        object: MathematicalObject::new(
            id,
            node.clone(),
            SemanticState {
                kind,
                interpretation,
                metadata: if immediately_usable {
                    ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
                } else {
                    ResultMetadata::unresolved(
                        Exactness::Unknown,
                        OutcomeReason::AlgorithmUncovered,
                    )
                },
                capabilities,
                requirements: Vec::new(),
            },
        ),
        form,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elaborates_nested_structural_and_application_nodes_without_reparsing() {
        let root = elaborate("D(x)(Sin(x)^2+Limit(t,0)(Sin(t)/t))").unwrap();
        assert!(matches!(root.form, MathematicalForm::Application { .. }));
        assert_eq!(root.children.len(), 2);
        assert!(matches!(
            root.children[1].form,
            MathematicalForm::Structural { .. }
        ));
        assert!(matches!(
            root.children[1].children[0].form,
            MathematicalForm::Structural { .. }
        ));
        assert!(matches!(
            root.children[1].children[1].form,
            MathematicalForm::Application { .. }
        ));
    }

    #[test]
    fn recognizes_only_unambiguous_missing_operand_forms_as_partials() {
        for source in ["D(x)", "Integrate(x)"] {
            let root = elaborate(source).unwrap();
            let partial = operand_partial(&root).unwrap().expect(source);
            assert!(matches!(
                partial.semantics.interpretation,
                SemanticInterpretation::PartialApplication(_)
            ));
        }
        let limit = elaborate("Limit(x,0)").unwrap();
        assert!(operand_partial(&limit).unwrap().is_none());
        let complete = elaborate("D(x)(x^2)").unwrap();
        assert!(operand_partial(&complete).unwrap().is_none());
        let invalid = elaborate("D(x+1)").unwrap();
        assert!(operand_partial(&invalid).is_err());
    }

    #[test]
    fn classifies_relations_collections_and_effects() {
        assert!(matches!(
            elaborate("x==1").unwrap().form,
            MathematicalForm::Relation { .. }
        ));
        assert!(matches!(
            elaborate("{x,1}").unwrap().form,
            MathematicalForm::Collection
        ));
        let plot = elaborate("Plot(x,x,0,1)").unwrap();
        assert!(matches!(
            plot.form,
            MathematicalForm::EffectApplication { .. }
        ));
        assert!(plot.object.semantics.capabilities == CapabilitySet::empty());
    }
}
