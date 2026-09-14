use std::rc::Rc;

use yacas_rs::value::LispObject;

use crate::engine::EngineError;
use crate::protocol::{ConditionSet, ResultMetadata};
use crate::semantic::ValueKind;

use super::super::object_state::{
    CapabilitySet, ExpressionPath, ExpressionView, MathematicalObject, ObjectDelta, Requirement,
    SemanticInterpretation, SemanticState,
};
use super::{
    operator_descriptor, ApplicationForm, ApplicationSlot, BinderScope, BoundArgument, OperatorId,
    PartialApplication, ResultTypeConstraint, ScopeArgument, TypedApplication, TypedArgument,
    ValueArgument,
};

pub(crate) fn signature_requirements(
    spelling: &str,
    arity: usize,
) -> Option<&'static [Requirement]> {
    operator_descriptor(spelling)?
        .slot_signatures
        .iter()
        .find(|signature| signature.arity == arity)
        .map(|signature| signature.requirements)
}

pub fn partial_application_state(
    spelling: &str,
    expected_arity: usize,
    bound_argument_count: usize,
) -> Result<PartialApplication, EngineError> {
    let descriptor = operator_descriptor(spelling)
        .ok_or_else(|| EngineError::InvalidInput(format!("未登记的偏应用运算符: {spelling}")))?;
    if bound_argument_count >= expected_arity || !descriptor.arities.contains(&expected_arity) {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 的 {expected_arity} 元签名不能绑定 {bound_argument_count} 个前缀参数"
        )));
    }
    let requirements = signature_requirements(spelling, expected_arity).ok_or_else(|| {
        EngineError::InvalidInput(format!(
            "{spelling} 的 {expected_arity} 元签名尚未声明参数槽类型"
        ))
    })?;
    let missing = requirements[bound_argument_count..].to_vec();
    Ok(PartialApplication {
        operator: descriptor.id,
        spelling: spelling.into(),
        expected_arity,
        bound_arguments: (0..bound_argument_count)
            .map(|slot| BoundArgument {
                slot,
                path: ExpressionPath::root().argument(slot),
            })
            .collect(),
        slots: requirements
            .iter()
            .enumerate()
            .map(|(slot, requirement)| {
                if slot < bound_argument_count {
                    ApplicationSlot::Bound {
                        slot,
                        path: ExpressionPath::root().argument(slot),
                    }
                } else {
                    ApplicationSlot::Missing {
                        slot,
                        requirement: requirement.clone(),
                    }
                }
            })
            .collect(),
        missing,
        binder_scopes: descriptor
            .binders
            .iter()
            .filter(|binder| {
                requirements.get(binder.binder_argument) == Some(&Requirement::Variable)
            })
            .map(|binder| BinderScope {
                binder_slot: binder.binder_argument,
                scope_slot: match binder.scope_argument {
                    ScopeArgument::First => 0,
                    ScopeArgument::Last => expected_arity - 1,
                    ScopeArgument::Index(index) => index,
                },
            })
            .collect(),
    })
}

pub fn typed_application_state(
    spelling: &str,
    arity: usize,
    expression: &Rc<LispObject>,
    conditions: ConditionSet,
) -> Result<TypedApplication, EngineError> {
    let descriptor = operator_descriptor(spelling)
        .ok_or_else(|| EngineError::InvalidInput(format!("未登记的运算符: {spelling}")))?;
    if !descriptor.arities.contains(&arity) {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 不支持 {arity} 个参数"
        )));
    }
    let requirements = signature_requirements(spelling, arity)
        .map(<[Requirement]>::to_vec)
        .unwrap_or_else(|| vec![Requirement::Operand; arity]);
    let binding = crate::binding::analyze_tree(expression);
    let result = match descriptor.id {
        OperatorId::Limit | OperatorId::MatrixTransform | OperatorId::MatrixAnalyze => {
            ResultTypeConstraint::DomainDefined
        }
        OperatorId::Derivative | OperatorId::Integral | OperatorId::Taylor => {
            ResultTypeConstraint::Expression
        }
        OperatorId::Factor | OperatorId::AlgebraTransform | OperatorId::Substitute => {
            ResultTypeConstraint::SameDomainAsOperand
        }
        _ => ResultTypeConstraint::DomainDefined,
    };
    Ok(TypedApplication {
        operator: descriptor.id,
        spelling: spelling.into(),
        arguments: requirements
            .into_iter()
            .enumerate()
            .map(|(slot, requirement)| TypedArgument {
                slot,
                path: ExpressionPath::root().argument(slot),
                requirement,
            })
            .collect(),
        binder_scopes: descriptor
            .binders
            .iter()
            .filter(|binder| {
                signature_requirements(spelling, arity).is_none_or(|items| {
                    items.get(binder.binder_argument) == Some(&Requirement::Variable)
                })
            })
            .map(|binder| BinderScope {
                binder_slot: binder.binder_argument,
                scope_slot: match binder.scope_argument {
                    ScopeArgument::First => 0,
                    ScopeArgument::Last => arity - 1,
                    ScopeArgument::Index(index) => index,
                },
            })
            .collect(),
        free_parameters: binding.free_symbols.into_iter().collect(),
        conditions,
        result,
    })
}

pub fn promote_held_application(
    spelling: &str,
    expression: &Rc<LispObject>,
    semantics: &mut SemanticState,
) -> Result<(), EngineError> {
    let arity =
        crate::input::with_parse_env(|env| ExpressionView::new(env, expression).arguments().len());
    let application = typed_application_state(
        spelling,
        arity,
        expression,
        semantics.metadata.conditions.clone(),
    )?;
    semantics.interpretation = SemanticInterpretation::HeldTypedApplication(application);
    Ok(())
}

pub fn promote_registered_held_expression(
    expression: &Rc<LispObject>,
    semantics: &mut SemanticState,
) -> Result<bool, EngineError> {
    let head = crate::input::with_parse_env(|env| {
        ExpressionView::new(env, expression)
            .head()
            .map(str::to_string)
    });
    let Some(head) = head.filter(|head| operator_descriptor(head).is_some()) else {
        return Ok(false);
    };
    promote_held_application(&head, expression, semantics)?;
    Ok(true)
}

pub fn operand_partial_state(
    spelling: &str,
    bound_argument_count: usize,
) -> Result<PartialApplication, EngineError> {
    let descriptor = operator_descriptor(spelling)
        .ok_or_else(|| EngineError::InvalidInput(format!("未登记的偏应用运算符: {spelling}")))?;
    if descriptor.value_argument != ValueArgument::Last
        || !descriptor.forms.contains(&ApplicationForm::Bodied)
    {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 不是可等待 operand 的 bodied 运算符"
        )));
    }
    let expected_arity = bound_argument_count + 1;
    if !descriptor.arities.contains(&expected_arity) {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 不支持绑定 {bound_argument_count} 个参数后等待 operand"
        )));
    }
    if signature_requirements(spelling, expected_arity).and_then(|requirements| requirements.last())
        != Some(&Requirement::Operand)
    {
        return Err(EngineError::InvalidInput(format!(
            "{spelling} 的该签名不是只等待 operand 的部分应用"
        )));
    }
    partial_application_state(spelling, expected_arity, bound_argument_count)
}

pub fn require_operand_partial(
    object: &MathematicalObject,
    expected: OperatorId,
) -> Result<&PartialApplication, EngineError> {
    let SemanticInterpretation::PartialApplication(partial) = &object.semantics.interpretation
    else {
        return Err(EngineError::InvalidInput(
            "对象不是等待 operand 的部分应用".into(),
        ));
    };
    if partial.operator != expected
        || partial.missing != [Requirement::Operand]
        || object.semantics.requirements != partial.missing
    {
        return Err(EngineError::InvalidInput(
            "部分应用的运算符或缺失槽位不匹配".into(),
        ));
    }
    if !matches!(
        partial.slots.as_slice(),
        [.., ApplicationSlot::Missing { slot, requirement: Requirement::Operand }]
            if *slot + 1 == partial.expected_arity
    ) {
        return Err(EngineError::InvalidInput("部分应用的参数槽状态无效".into()));
    }
    crate::input::with_parse_env(|env| {
        let view = object.view(env);
        if view.head() != Some(partial.spelling.as_str())
            || view.arguments().len() != partial.bound_arguments.len()
        {
            return Err(EngineError::Parse(
                "部分应用语义状态与保留的 AST 不一致".into(),
            ));
        }
        Ok(())
    })?;
    Ok(partial)
}

/// Fill the missing operand slot by linking the operand AST into the retained
/// application. This constructs an application object; domain evaluation is a
/// separate transition.
pub fn complete_operand_partial(
    partial_object: &MathematicalObject,
    operand: &MathematicalObject,
) -> Result<MathematicalObject, EngineError> {
    let SemanticInterpretation::PartialApplication(partial) =
        &partial_object.semantics.interpretation
    else {
        return Err(EngineError::InvalidInput("对象不是部分应用".into()));
    };
    require_operand_partial(partial_object, partial.operator)?;
    fill_next_partial_argument(partial_object, operand)
}

pub fn fill_next_partial_argument(
    partial_object: &MathematicalObject,
    argument: &MathematicalObject,
) -> Result<MathematicalObject, EngineError> {
    let SemanticInterpretation::PartialApplication(partial) =
        &partial_object.semantics.interpretation
    else {
        return Err(EngineError::InvalidInput("对象不是部分应用".into()));
    };
    let (missing_slot, requirement) = partial
        .slots
        .iter()
        .find_map(|slot| match slot {
            ApplicationSlot::Missing { slot, requirement } => Some((*slot, requirement.clone())),
            _ => None,
        })
        .ok_or_else(|| EngineError::InvalidInput("部分应用没有匹配的缺失参数槽".into()))?;
    if matches!(requirement, Requirement::Variable) {
        let is_symbol = crate::input::with_parse_env(|env| argument.view(env).atom().is_some());
        if !is_symbol {
            return Err(EngineError::InvalidInput("变量参数槽必须填入符号".into()));
        }
    }
    if matches!(requirement, Requirement::Order) && argument.print_source().parse::<u32>().is_err()
    {
        return Err(EngineError::InvalidInput(
            "阶数参数槽必须填入非负整数".into(),
        ));
    }
    let expression = partial_object.append_application_argument(argument)?;
    let mut completed = partial_object.clone();
    let remaining = partial.missing[1..].to_vec();
    let interpretation = if remaining.is_empty() {
        SemanticInterpretation::TypedApplication(typed_application_state(
            &partial.spelling,
            partial.expected_arity,
            &expression,
            partial_object.semantics.metadata.conditions.clone(),
        )?)
    } else {
        let mut next = partial.clone();
        next.bound_arguments.push(BoundArgument {
            slot: missing_slot,
            path: ExpressionPath::root().argument(missing_slot),
        });
        next.slots[missing_slot] = ApplicationSlot::Bound {
            slot: missing_slot,
            path: ExpressionPath::root().argument(missing_slot),
        };
        next.missing = remaining.clone();
        SemanticInterpretation::PartialApplication(next)
    };
    completed.apply(ObjectDelta {
        expression: Some(expression),
        semantics: Some(SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation,
            metadata: ResultMetadata::unresolved(
                crate::semantic::Exactness::Symbolic,
                crate::protocol::OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: remaining,
        }),
        overlay: None,
        normalization: None,
    });
    Ok(completed)
}
