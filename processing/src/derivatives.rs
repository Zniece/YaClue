//! Object-native differentiation over the existing, tested `StepsD'Full`
//! decision program.  The script remains the algorithm fact source while this
//! module owns semantic state transitions and rule-trace construction.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{validate_expression, validate_symbol};
use crate::protocol::{Condition, ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, ExpressionView,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectDelta, ObjectId,
    OperatorId, Requirement, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticOperation, SemanticState,
};
use crate::steps::{Step, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivativeStatus {
    Computed,
    Unresolved,
}

/// JSON/product projection of the derivative's semantic conclusion.  It is
/// deliberately derived from the computation object, never from a Step.
#[derive(Debug, Clone, Serialize)]
pub struct DerivativeResult {
    pub status: DerivativeStatus,
    pub expression: String,
    pub variable: String,
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivativeRequest {
    pub variable: String,
    pub order: u32,
}

impl DerivativeRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol(&self.variable, "求导变量")?;
        if self.order == 0 {
            return Err(EngineError::InvalidInput("求导阶数必须 >= 1".into()));
        }
        Ok(())
    }
}

/// Curried `D(x[, order])`, waiting for an operand in the semantic data plane.
pub fn derivative_partial(
    id: ObjectId,
    request: &DerivativeRequest,
) -> Result<crate::semantic_core::MathematicalObject, EngineError> {
    request.validate()?;
    let source = if request.order == 1 {
        format!("D({})", request.variable)
    } else {
        format!("D({},{})", request.variable, request.order)
    };
    object_from_source(
        id,
        &source,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::PartialApplication(
                crate::semantic_core::operand_partial_state(
                    "D",
                    if request.order == 1 { 1 } else { 2 },
                )?,
            ),
            metadata: ResultMetadata::unresolved(
                Exactness::Symbolic,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: vec![Requirement::Operand],
        },
    )
}

pub fn apply_derivative_partial(
    engine: &mut dyn Engine,
    partial: &crate::semantic_core::MathematicalObject,
    operand: &crate::semantic_core::MathematicalObject,
) -> Result<Computation, EngineError> {
    crate::semantic_core::require_operand_partial(partial, OperatorId::Derivative)?;
    let _application = crate::semantic_core::complete_operand_partial(partial, operand)?;
    let request = crate::input::with_parse_env(|env| {
        let view = partial.view(env);
        if view.head() != Some("D") {
            return Err(EngineError::Parse("D 部分应用 AST 形态异常".into()));
        }
        let arguments = view.arguments();
        let order = match arguments.as_slice() {
            [variable] => {
                return Ok(DerivativeRequest {
                    variable: variable.print_source(),
                    order: 1,
                });
            }
            [_variable, order] => order
                .print_source()
                .parse()
                .map_err(|_| EngineError::Parse("D 部分应用阶数异常".into()))?,
            _ => return Err(EngineError::Parse("D 部分应用参数数量异常".into())),
        };
        Ok(DerivativeRequest {
            variable: arguments[0].print_source(),
            order,
        })
    })?;
    DerivativeOperation.compute(engine, operand, &request)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DerivativeOperation;

impl SemanticOperation<DerivativeRequest> for DerivativeOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &DerivativeRequest,
    ) -> Result<Computation, EngineError> {
        derivative_computation_for_object(engine, input, request)
    }
}

#[derive(Clone)]
struct DerivativeFact {
    rule: String,
    expression: String,
    explanation: String,
    importance: RuleImportance,
}

fn facts(
    engine: &mut dyn Engine,
    expression: &str,
    request: &DerivativeRequest,
) -> Result<Vec<DerivativeFact>, EngineError> {
    let command = format!(
        "StepsD'Full({expression},{},{})",
        request.variable, request.order
    );
    let Expr::Call { head, args } = engine.eval_expr(&command)? else {
        return Err(EngineError::Parse("求导步骤事实不是列表".into()));
    };
    if head != "List" || args.is_empty() {
        return Err(EngineError::Eval("未能生成求导步骤".into()));
    }
    args.into_iter()
        .enumerate()
        .map(|(index, item)| {
            let Expr::Call { head, args } = item else {
                return Err(EngineError::Parse(format!("求导步骤事件 {index} 不是列表")));
            };
            if head != "List" || args.len() != 4 {
                return Err(EngineError::Parse(format!(
                    "求导步骤事件 {index} 必须是四字段 List"
                )));
            }
            let text = |item: &Expr, field| match item {
                Expr::Symbol(value)
                    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') =>
                {
                    Ok(value[1..value.len() - 1].to_owned())
                }
                _ => Err(EngineError::Parse(format!(
                    "求导步骤事件 {index} 的 {field} 必须是字符串"
                ))),
            };
            let importance = match &args[3] {
                Expr::Number(value) if value == "0" => RuleImportance::Routine,
                Expr::Number(value) if value == "1" => RuleImportance::Normal,
                Expr::Number(value) if value == "2" => RuleImportance::Key,
                _ => {
                    return Err(EngineError::Parse(format!(
                        "求导步骤事件 {index} 的 importance 必须是 0、1 或 2"
                    )))
                }
            };
            Ok(DerivativeFact {
                rule: text(&args[0], "rule")?,
                expression: args[1].to_string(),
                explanation: text(&args[2], "explanation")?,
                importance,
            })
        })
        .collect()
}

pub fn derivative_computation(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
) -> Result<Computation, EngineError> {
    validate_expression(expression, "求导表达式")?;
    let input = object_from_source(
        ObjectId(1),
        expression,
        SemanticState {
            kind: ValueKind::Expression,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::unresolved(
                Exactness::Unknown,
                OutcomeReason::AlgorithmUncovered,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        },
    )?;
    derivative_computation_for_object(
        engine,
        &input,
        &DerivativeRequest {
            variable: variable.into(),
            order,
        },
    )
}

pub fn derivative_computation_for_object(
    engine: &mut dyn Engine,
    operand: &crate::semantic_core::MathematicalObject,
    request: &DerivativeRequest,
) -> Result<Computation, EngineError> {
    request.validate()?;
    if !operand
        .semantics
        .capabilities
        .contains(crate::semantic_core::ObjectCapability::Differentiate)
    {
        return Err(EngineError::InvalidInput("该数学对象不具备求导能力".into()));
    }
    if let Some(computation) = derivative_of_typed_integral(engine, operand, request)? {
        return Ok(computation);
    }
    if let Some(computation) = derivative_of_registered_function(engine, operand, request)? {
        return Ok(computation);
    }
    let source = operand.print_source();
    let facts = facts(engine, &source, request)?;
    let final_expression = facts
        .last()
        .expect("checked nonempty facts")
        .expression
        .clone();
    let unresolved = crate::input::with_parse_env(|env| {
        let tree = yacas_rs::parser::parse_expression(env, &format!("{final_expression};"))
            .map_err(|error| EngineError::Parse(format!("求导结果语法异常: {error:?}")))?
            .ok_or_else(|| EngineError::Parse("求导结果为空".into()))?;
        Ok(ExpressionView::new(env, &tree)
            .head()
            .is_some_and(|head| matches!(head, "D" | "Deriv")))
    })?;
    let mut metadata = if unresolved {
        ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
    } else {
        ResultMetadata::solved(
            Exactness::Symbolic,
            operand.semantics.metadata.conditions.clone(),
        )
    };
    if unresolved {
        metadata.conditions = operand.semantics.metadata.conditions.clone();
    }
    let mut output_object = operand.clone();
    let output_source = if unresolved {
        format!("D({},{})({source})", request.variable, request.order)
    } else {
        final_expression.clone()
    };
    let mut semantics = SemanticState {
        kind: ValueKind::Unevaluated,
        interpretation: if unresolved {
            SemanticInterpretation::HeldApplication {
                operator: "D".into(),
            }
        } else {
            SemanticInterpretation::PlainExpression
        },
        metadata,
        capabilities: CapabilitySet::symbolic_expression(),
        requirements: Vec::new(),
    };
    let output_ast =
        crate::semantic_core::parse_engine_expression(&output_source)?.raw_expression();
    if unresolved {
        crate::semantic_core::promote_held_application("D", &output_ast, &mut semantics)?;
    } else {
        semantics.kind = crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &output_ast)
                .semantic
                .kind
        });
    }
    let input_ref = output_object.reference(None);
    output_object.apply(ObjectDelta {
        expression: Some(output_ast),
        semantics: Some(semantics),
        overlay: None,
        normalization: (!unresolved).then_some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: Vec::new(),
            mode: NormalizationMode::Operation(OperatorId::Derivative),
        }),
    });
    let output_ref = output_object.reference(None);
    let last = facts.len() - 1;
    let events = facts
        .into_iter()
        .enumerate()
        .map(|(index, fact)| RuleEvent {
            // Preserve the established rule vocabulary at the compatibility
            // projection boundary.  The trace itself supplies the missing
            // object/revision semantics without inventing display-only names.
            rule: fact.rule,
            input: input_ref.clone(),
            additional_inputs: Vec::new(),
            output: output_ref.clone(),
            bindings: vec![
                ("variable".into(), request.variable.clone()),
                ("order".into(), request.order.to_string()),
            ],
            conditions: operand.semantics.metadata.conditions.conditions().to_vec(),
            payload: if index == last {
                RulePayload::Rewrite
            } else {
                RulePayload::Structural
            },
            importance: if index == last {
                RuleImportance::Key
            } else {
                fact.importance
            },
            presentation: Some(RulePresentation {
                expression: fact.expression,
                explanation: fact.explanation,
                tex_override: None,
            }),
        })
        .collect();
    Ok(Computation {
        output: if unresolved {
            ComputationOutput::Held(output_object)
        } else {
            ComputationOutput::Value(output_object)
        },
        trace: Some(RuleTrace { events }),
        certificates: Vec::new(),
        effects: Vec::new(),
    })
}

struct FunctionPartialDerivative {
    expression: String,
    conditions: Vec<Condition>,
    requires_structural_output: bool,
}

type PartialDerivativeBuilder = fn(&[String], usize) -> Option<FunctionPartialDerivative>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FunctionParameterRole {
    Argument,
    ContinuousParameter,
    Order,
}

impl FunctionParameterRole {
    const fn label(self) -> &'static str {
        match self {
            Self::Argument => "argument",
            Self::ContinuousParameter => "continuous_parameter",
            Self::Order => "order",
        }
    }
}

#[derive(Clone, Copy)]
struct FunctionDerivativeRule {
    id: &'static str,
    head: &'static str,
    arity: usize,
    parameter_roles: &'static [FunctionParameterRole],
    partial_derivative: PartialDerivativeBuilder,
}

pub(crate) fn preserves_registered_function_identity(head: &str, arity: usize) -> bool {
    FUNCTION_DERIVATIVE_RULES
        .iter()
        .any(|rule| rule.head == head && rule.arity == arity)
}

fn gamma_partial(arguments: &[String], argument_index: usize) -> Option<FunctionPartialDerivative> {
    (argument_index == 0).then(|| {
        let argument = &arguments[0];
        FunctionPartialDerivative {
            expression: format!("Gamma({argument})*PolyGamma(0,{argument})"),
            conditions: Vec::new(),
            requires_structural_output: false,
        }
    })
}

fn erf_partial(arguments: &[String], argument_index: usize) -> Option<FunctionPartialDerivative> {
    (argument_index == 0).then(|| FunctionPartialDerivative {
        expression: format!("2*Exp(-(({})^2))/Sqrt(Pi)", arguments[0]),
        conditions: Vec::new(),
        requires_structural_output: false,
    })
}

fn poly_gamma_partial(
    arguments: &[String],
    argument_index: usize,
) -> Option<FunctionPartialDerivative> {
    let order = &arguments[0];
    let conditions = if order.parse::<u64>().is_ok() {
        Vec::new()
    } else {
        vec![
            Condition::Integer {
                expression: order.clone(),
            },
            Condition::Unknown {
                description: format!("PolyGamma 阶数 {order} 必须非负"),
            },
        ]
    };
    (argument_index == 1).then(|| FunctionPartialDerivative {
        expression: format!("PolyGamma(({order})+1,{})", arguments[1]),
        conditions,
        requires_structural_output: false,
    })
}

fn lambert_w_partial(
    arguments: &[String],
    argument_index: usize,
) -> Option<FunctionPartialDerivative> {
    (argument_index == 0).then(|| {
        let function = format!("LambertW({})", arguments[0]);
        FunctionPartialDerivative {
            // This equivalent form is regular at zero, unlike W(z)/(z(1+W(z))).
            expression: format!("1/(Exp({function})*(1+{function}))"),
            conditions: vec![Condition::NonZero {
                expression: format!("1+{function}"),
            }],
            requires_structural_output: false,
        }
    })
}

fn beta_partial(arguments: &[String], argument_index: usize) -> Option<FunctionPartialDerivative> {
    let [left, right] = arguments else {
        return None;
    };
    let active = &arguments[argument_index];
    Some(FunctionPartialDerivative {
        expression: format!(
            "Beta({left},{right})*(PolyGamma(0,{active})-PolyGamma(0,({left})+({right})))"
        ),
        conditions: vec![
            Condition::RealPartPositive {
                expression: left.clone(),
            },
            Condition::RealPartPositive {
                expression: right.clone(),
            },
        ],
        // Yacas eagerly rewrites Beta to a Gamma quotient. Keep the compact
        // registered function identity in the derivative result.
        requires_structural_output: true,
    })
}

fn incomplete_gamma_partial(
    arguments: &[String],
    argument_index: usize,
) -> Option<FunctionPartialDerivative> {
    let [limit, shape] = arguments else {
        return None;
    };
    let conditions = vec![
        Condition::RealPartPositive {
            expression: shape.clone(),
        },
        Condition::Positive {
            expression: limit.clone(),
        },
    ];
    match argument_index {
        0 => Some(FunctionPartialDerivative {
            expression: format!("Exp((({shape})-1)*Ln({limit})-({limit}))"),
            conditions,
            requires_structural_output: false,
        }),
        1 => {
            let binder = ["t", "u", "v", "s", "r"]
                .into_iter()
                .find(|candidate| !limit.contains(candidate) && !shape.contains(candidate))
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    crate::input::fresh_internal_symbols(
                        "IncompleteGammaDerivative",
                        &[limit, shape],
                        ["T"],
                    )[0]
                    .clone()
                });
            Some(FunctionPartialDerivative {
                expression: format!(
                    "Integrate({binder},0,{limit})(Exp((({shape})-1)*Ln({binder})-{binder})*Ln({binder}))"
                ),
                conditions,
                requires_structural_output: true,
            })
        }
        _ => None,
    }
}

fn bessel_j_partial(
    arguments: &[String],
    argument_index: usize,
) -> Option<FunctionPartialDerivative> {
    let [order, argument] = arguments else {
        return None;
    };
    (argument_index == 1).then(|| FunctionPartialDerivative {
        expression: format!("(BesselJ(({order})-1,{argument})-BesselJ(({order})+1,{argument}))/2"),
        conditions: Vec::new(),
        // Adjacent orders are the canonical derivative representation; do not
        // let numeric/special-case engine rules expand half-integer orders.
        requires_structural_output: true,
    })
}

/// Function identities are data in this bounded registry. The dispatcher
/// owns chain-rule composition; adding Erf, Beta or Bessel derivatives does
/// not add branches to `DerivativeOperation`.
const FUNCTION_DERIVATIVE_RULES: &[FunctionDerivativeRule] = &[
    FunctionDerivativeRule {
        id: "function.gamma.derivative",
        head: "Gamma",
        arity: 1,
        parameter_roles: &[FunctionParameterRole::Argument],
        partial_derivative: gamma_partial,
    },
    FunctionDerivativeRule {
        id: "function.erf.derivative",
        head: "Erf",
        arity: 1,
        parameter_roles: &[FunctionParameterRole::Argument],
        partial_derivative: erf_partial,
    },
    FunctionDerivativeRule {
        id: "function.poly-gamma.derivative",
        head: "PolyGamma",
        arity: 2,
        parameter_roles: &[
            FunctionParameterRole::Order,
            FunctionParameterRole::Argument,
        ],
        partial_derivative: poly_gamma_partial,
    },
    FunctionDerivativeRule {
        id: "function.lambert-w.derivative",
        head: "LambertW",
        arity: 1,
        parameter_roles: &[FunctionParameterRole::Argument],
        partial_derivative: lambert_w_partial,
    },
    FunctionDerivativeRule {
        id: "function.beta.derivative",
        head: "Beta",
        arity: 2,
        parameter_roles: &[
            FunctionParameterRole::ContinuousParameter,
            FunctionParameterRole::ContinuousParameter,
        ],
        partial_derivative: beta_partial,
    },
    FunctionDerivativeRule {
        id: "function.incomplete-gamma.derivative",
        head: "IncompleteGamma",
        arity: 2,
        parameter_roles: &[
            FunctionParameterRole::Argument,
            FunctionParameterRole::ContinuousParameter,
        ],
        partial_derivative: incomplete_gamma_partial,
    },
    FunctionDerivativeRule {
        id: "function.bessel-j.derivative",
        head: "BesselJ",
        arity: 2,
        parameter_roles: &[
            FunctionParameterRole::Order,
            FunctionParameterRole::Argument,
        ],
        partial_derivative: bessel_j_partial,
    },
];

fn derivative_of_registered_function(
    engine: &mut dyn Engine,
    operand: &crate::semantic_core::MathematicalObject,
    request: &DerivativeRequest,
) -> Result<Option<Computation>, EngineError> {
    if request.order != 1 {
        return Ok(None);
    }
    let function = crate::input::with_parse_env(|env| {
        let view = operand.view(env);
        Ok(view.head().map(|head| {
            (
                head.to_owned(),
                view.arguments()
                    .into_iter()
                    .map(|argument| argument.print_source())
                    .collect::<Vec<_>>(),
            )
        }))
    })?;
    let Some((head, arguments)) = function else {
        return Ok(None);
    };
    let Some(rule) = FUNCTION_DERIVATIVE_RULES
        .iter()
        .find(|rule| rule.head == head && rule.arity == arguments.len())
    else {
        return Ok(None);
    };
    debug_assert_eq!(rule.parameter_roles.len(), rule.arity);
    let mut terms = Vec::with_capacity(arguments.len());
    let mut events = Vec::new();
    let mut certificates = Vec::new();
    let mut effects = Vec::new();
    let mut requires_structural_output = false;
    let mut condition_items = operand.semantics.metadata.conditions.conditions().to_vec();
    for (index, argument) in arguments.iter().enumerate() {
        let mut inner = derivative_computation(engine, argument, &request.variable, 1)?;
        let Some(inner_value) = inner.value() else {
            return Ok(None);
        };
        let inner_source = inner_value.print_source();
        let Some(partial) = (rule.partial_derivative)(&arguments, index) else {
            // A missing partial marks a discrete or otherwise unsupported slot,
            // not an implicit zero. It is safe to omit only for a constant slot.
            if inner_source != "0" {
                return Ok(None);
            }
            continue;
        };
        if inner_source != "0" {
            requires_structural_output |= partial.requires_structural_output;
            terms.push(if inner_source == "1" {
                partial.expression.clone()
            } else {
                format!("({})*({inner_source})", partial.expression)
            });
            condition_items.extend(partial.conditions);
        }
        condition_items.extend(
            inner_value
                .semantics
                .metadata
                .conditions
                .conditions()
                .iter()
                .cloned(),
        );
        events.extend(
            inner
                .trace
                .take()
                .map(|trace| trace.events)
                .unwrap_or_default(),
        );
        certificates.append(&mut inner.certificates);
        effects.append(&mut inner.effects);
    }
    let source = if terms.is_empty() {
        "0".into()
    } else {
        terms.join("+")
    };
    let source = if requires_structural_output {
        source
    } else {
        engine.eval_expr(&source)?.to_string()
    };
    let conditions = ConditionSet::new(condition_items)?;
    let mut output = operand.clone();
    let semantics = SemanticState {
        kind: ValueKind::Expression,
        interpretation: SemanticInterpretation::PlainExpression,
        metadata: ResultMetadata::solved(Exactness::Symbolic, conditions.clone()),
        capabilities: CapabilitySet::symbolic_expression(),
        requirements: Vec::new(),
    };
    let parsed = crate::semantic_core::parse_engine_expression(&source)?;
    let input_ref = output.reference(None);
    output.apply(ObjectDelta {
        expression: Some(parsed.raw_expression()),
        semantics: Some(semantics),
        overlay: None,
        normalization: Some(NormalizationMetadata {
            level: NormalizationLevel::Domain,
            assumptions: conditions.conditions().to_vec(),
            mode: NormalizationMode::Operation(OperatorId::Derivative),
        }),
    });
    let event = RuleEvent {
        rule: "derivative-registered-function-chain-rule".into(),
        input: input_ref,
        additional_inputs: Vec::new(),
        output: output.reference(None),
        bindings: vec![
            ("variable".into(), request.variable.clone()),
            ("function".into(), head),
            ("derivative_rule".into(), rule.id.into()),
            (
                "parameter_roles".into(),
                rule.parameter_roles
                    .iter()
                    .map(|role| role.label())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ],
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "应用已登记的函数偏导规则，并由通用链式法则组合参数导数。".into(),
            tex_override: None,
        }),
    };
    events.push(event);
    Ok(Some(Computation {
        output: ComputationOutput::Value(output),
        trace: Some(RuleTrace { events }),
        certificates,
        effects,
    }))
}

fn derivative_of_typed_integral(
    _engine: &mut dyn Engine,
    operand: &crate::semantic_core::MathematicalObject,
    request: &DerivativeRequest,
) -> Result<Option<Computation>, EngineError> {
    let application = match &operand.semantics.interpretation {
        SemanticInterpretation::TypedApplication(application)
        | SemanticInterpretation::HeldTypedApplication(application)
            if application.operator == OperatorId::Integral =>
        {
            application
        }
        _ => return Ok(None),
    };
    let source_for = |requirement: Requirement| -> Result<Option<String>, EngineError> {
        let Some(argument) = application
            .arguments
            .iter()
            .find(|argument| argument.requirement == requirement)
        else {
            return Ok(None);
        };
        crate::input::with_parse_env(|env| {
            operand
                .view(env)
                .at_path(&argument.path)
                .map(|view| view.print_source())
                .ok_or_else(|| EngineError::Parse("积分 typed slot 的 AST 路径失效".into()))
                .map(Some)
        })
    };
    let Some(variable) = source_for(Requirement::Variable)? else {
        return Ok(None);
    };
    let Some(integrand) = source_for(Requirement::Operand)? else {
        return Ok(None);
    };
    if variable == request.variable && source_for(Requirement::LowerBound)?.is_none() {
        let semantics = SemanticState {
            kind: ValueKind::Expression,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::solved(
                Exactness::Symbolic,
                operand.semantics.metadata.conditions.clone(),
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let parsed = crate::semantic_core::parse_engine_expression(&integrand)?;
        let mut output = operand.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: Some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: operand.semantics.metadata.conditions.conditions().to_vec(),
                mode: NormalizationMode::Operation(OperatorId::Derivative),
            }),
        });
        return Ok(Some(integral_derivative_computation(
            operand,
            output,
            "derivative-of-indefinite-integral",
            "同变量求导与不定积分相消。",
            operand.semantics.metadata.conditions.clone(),
            false,
        )));
    }
    let lower = source_for(Requirement::LowerBound)?;
    let upper = source_for(Requirement::UpperBound)?;
    for bound in [&lower, &upper].into_iter().flatten() {
        if crate::binding::analyze(bound)?
            .free_symbols
            .contains(&request.variable)
        {
            // Variable bounds require the full Leibniz boundary terms. Keep
            // the original derivative held until that typed rule is added.
            return Ok(None);
        }
    }
    // This is a semantic degradation, not an invitation to force the inner
    // derivative through a different algorithm. Retaining the typed
    // derivative preserves exact composition until its own rule is known.
    let derivative = format!("D({},{})({integrand})", request.variable, request.order);
    let source = match (lower, upper) {
        (Some(lower), Some(upper)) => {
            format!("Integrate({variable},{lower},{upper})({derivative})")
        }
        (None, None) => format!("Integrate({variable})({derivative})"),
        _ => return Ok(None),
    };
    let condition = Condition::Unknown {
        description: format!("允许对参数 {} 在积分号下求导", request.variable),
    };
    let conditions = ConditionSet::new(
        operand
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .cloned()
            .chain(std::iter::once(condition)),
    )?;
    let mut metadata =
        ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::ConditionInsufficient);
    metadata.conditions = conditions.clone();
    let mut semantics = SemanticState {
        kind: ValueKind::Unevaluated,
        interpretation: SemanticInterpretation::HeldApplication {
            operator: "Integrate".into(),
        },
        metadata,
        capabilities: CapabilitySet::symbolic_expression(),
        requirements: Vec::new(),
    };
    let parsed = crate::semantic_core::parse_engine_expression(&source)?;
    crate::semantic_core::promote_held_application(
        "Integrate",
        &parsed.raw_expression(),
        &mut semantics,
    )?;
    let mut output = operand.clone();
    output.apply(ObjectDelta {
        expression: Some(parsed.raw_expression()),
        semantics: Some(semantics),
        overlay: None,
        normalization: None,
    });
    let computation = integral_derivative_computation(
        operand,
        output,
        "differentiate-under-integral-sign",
        "在显式正则性条件下，将参数求导保留到积分号内。",
        conditions,
        true,
    );
    Ok(Some(computation))
}

fn integral_derivative_computation(
    input: &crate::semantic_core::MathematicalObject,
    output: crate::semantic_core::MathematicalObject,
    rule: &str,
    explanation: &str,
    conditions: ConditionSet,
    held: bool,
) -> Computation {
    let event = RuleEvent {
        rule: rule.into(),
        input: input.reference(None),
        additional_inputs: Vec::new(),
        output: output.reference(None),
        bindings: Vec::new(),
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: explanation.into(),
            tex_override: None,
        }),
    };
    Computation {
        output: if held {
            ComputationOutput::Held(output)
        } else {
            ComputationOutput::Value(output)
        },
        trace: Some(RuleTrace {
            events: vec![event],
        }),
        certificates: Vec::new(),
        effects: Vec::new(),
    }
}

pub fn derivative_result(
    computation: &Computation,
    request: &DerivativeRequest,
) -> DerivativeResult {
    let object = computation
        .subject()
        .expect("derivative computations always produce an object");
    DerivativeResult {
        status: match &computation.output {
            ComputationOutput::Held(_) => DerivativeStatus::Unresolved,
            ComputationOutput::Value(_) => DerivativeStatus::Computed,
            ComputationOutput::NoValue(_) | ComputationOutput::EffectsOnly => {
                unreachable!("derivative never produces an absent/effect-only result")
            }
        },
        expression: object.print_source(),
        variable: request.variable.clone(),
        order: request.order,
    }
}

pub fn derivative_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let computation = derivative_computation(engine, expression, variable, order)?;
    crate::steps::render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .expect("derivative always emits trace"),
        verbosity,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{object_from_source, ObjectId};

    #[test]
    fn semantic_operation_preserves_object_identity_and_projects_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        let computation = derivative_computation(&mut engine, "Sin(x)^2", "x", 1).unwrap();
        assert!(matches!(computation.output, ComputationOutput::Value(_)));
        assert_eq!(
            computation.value().unwrap().print_source(),
            "2*Sin(x)*Cos(x)"
        );
        assert_eq!(computation.value().unwrap().revision.0, 1);
        assert_eq!(
            computation
                .value()
                .unwrap()
                .normalization
                .as_ref()
                .unwrap()
                .metadata
                .level,
            NormalizationLevel::Domain
        );
        let steps = crate::steps::render_rule_trace(
            &mut engine,
            computation.trace.as_ref().unwrap(),
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(steps.last().unwrap().expr, "(2 * (Sin(x) * Cos(x)))");
    }

    #[test]
    fn partial_derivative_recovers_request_from_ast() {
        let mut engine = RustEngine::spawn().unwrap();
        let partial = derivative_partial(
            ObjectId(9),
            &DerivativeRequest {
                variable: "x".into(),
                order: 1,
            },
        )
        .unwrap();
        let operand = object_from_source(
            ObjectId(42),
            "x^2",
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: ResultMetadata::unresolved(
                    Exactness::Unknown,
                    OutcomeReason::AlgorithmUncovered,
                ),
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap();
        let computation = apply_derivative_partial(&mut engine, &partial, &operand).unwrap();
        assert_eq!(computation.value().unwrap().id, ObjectId(42));
        assert_eq!(computation.value().unwrap().print_source(), "2*x");
    }

    #[test]
    fn differentiates_a_limit_value_as_the_same_mathematical_object() {
        let mut engine = RustEngine::spawn().unwrap();
        let limit = crate::limits::limit_computation(
            &mut engine,
            "Sin(t)/t+x^2",
            "t",
            "0",
            crate::limits::LimitDirection::Both,
        )
        .unwrap();
        let limited = limit.value().expect("this limit is solved");
        let derivative = DerivativeOperation
            .compute(
                &mut engine,
                limited,
                &DerivativeRequest {
                    variable: "x".into(),
                    order: 1,
                },
            )
            .unwrap();
        assert_eq!(derivative.value().unwrap().id, limited.id);
        assert_eq!(
            derivative.value().unwrap().revision.0,
            limited.revision.0 + 1
        );
        assert_eq!(derivative.value().unwrap().print_source(), "2*x");
        assert_eq!(
            derivative
                .value()
                .unwrap()
                .normalization
                .as_ref()
                .unwrap()
                .revision,
            derivative.value().unwrap().revision
        );
    }

    #[test]
    fn derivative_result_kind_is_derived_from_the_result_ast() {
        let mut engine = RustEngine::spawn().unwrap();
        let constant = derivative_computation(&mut engine, "x", "x", 1).unwrap();
        assert_eq!(constant.value().unwrap().print_source(), "1");
        assert_eq!(constant.value().unwrap().semantics.kind, ValueKind::Scalar);

        let expression = derivative_computation(&mut engine, "x^2", "x", 1).unwrap();
        assert_eq!(
            expression.value().unwrap().semantics.kind,
            ValueKind::Expression
        );
    }

    #[test]
    fn differentiates_native_gamma_and_preserves_lowering_conditions() {
        let mut engine = RustEngine::spawn().unwrap();
        let direct = crate::elaboration::elaborate("D(x)Gamma(x)").unwrap();
        let result = crate::arithmetic::execute_elaborated_structure(&mut engine, &direct).unwrap();
        assert_eq!(
            result.value().unwrap().print_source().replace(' ', ""),
            "Gamma(x)*PolyGamma(0,x)"
        );
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| { event.rule == "derivative-registered-function-chain-rule" }));

        let chained = crate::elaboration::elaborate("D(x)Gamma(x^2)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &chained).unwrap();
        let source = result.value().unwrap().print_source().replace(' ', "");
        assert!(source.contains("Gamma(x^2)*PolyGamma(0,x^2)"));
        assert!(source.contains("2*x"));

        let lowered =
            crate::elaboration::elaborate("D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t)))")
                .unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &lowered).unwrap();
        assert_eq!(
            result.value().unwrap().print_source().replace(' ', ""),
            "Gamma(x)*PolyGamma(0,x)"
        );
        assert!(matches!(
            result.value().unwrap().semantics.metadata.conditions.conditions(),
            [Condition::RealPartPositive { expression }] if expression == "x"
        ));
        let rules = result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .map(|event| event.rule.as_str())
            .collect::<Vec<_>>();
        assert!(rules.contains(&"intrinsic-gamma-lowering"));
        assert!(rules.contains(&"derivative-registered-function-chain-rule"));
    }

    #[test]
    fn differentiates_registered_single_active_argument_special_functions() {
        let mut engine = RustEngine::spawn().unwrap();
        let cases = [
            ("D(x)Erf(x^2)", "function.erf.derivative"),
            ("D(x)PolyGamma(2,Sin(x))", "function.poly-gamma.derivative"),
            ("D(x)LambertW(Exp(x))", "function.lambert-w.derivative"),
        ];

        for (source, rule_id) in cases {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let result =
                crate::arithmetic::execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            let value = result.value().expect(source);
            assert!(
                !value.print_source().contains("D("),
                "{source}: {}",
                value.print_source()
            );
            assert!(result.trace.as_ref().unwrap().events.iter().any(|event| {
                event.rule == "derivative-registered-function-chain-rule"
                    && event
                        .bindings
                        .iter()
                        .any(|(key, value)| key == "derivative_rule" && value == rule_id)
            }));
        }
    }

    #[test]
    fn special_function_rules_preserve_conditions_and_reject_varying_discrete_slots() {
        let mut engine = RustEngine::spawn().unwrap();
        let lambert = crate::elaboration::elaborate("D(x)LambertW(x)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &lambert).unwrap();
        assert!(result
            .value()
            .unwrap()
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, Condition::NonZero { expression } if expression.contains("1+LambertW(x)"))));

        let poly_gamma = crate::elaboration::elaborate("D(x)PolyGamma(n,x)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &poly_gamma).unwrap();
        assert!(result
            .value()
            .unwrap()
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, Condition::Integer { expression } if expression == "n")));

        let varying_order = crate::elaboration::elaborate("D(x)PolyGamma(x,x)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &varying_order).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|event| event.rule != "derivative-registered-function-chain-rule"));
    }

    #[test]
    fn multivariate_special_function_rules_sum_every_active_parameter_slot() {
        let mut engine = RustEngine::spawn().unwrap();
        let beta = crate::elaboration::elaborate("D(x)Beta(x,x^2)").unwrap();
        let beta_result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &beta).unwrap();
        let beta_source = beta_result.value().unwrap().print_source();
        assert!(beta_source.contains("PolyGamma(0,x)"), "{beta_source}");
        assert!(beta_source.contains("PolyGamma(0,x^2)"), "{beta_source}");
        assert!(beta_source.contains("2*x"), "{beta_source}");
        assert!(beta_result
            .value()
            .unwrap()
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, Condition::RealPartPositive { expression } if expression == "x^2")));

        let incomplete = crate::elaboration::elaborate("D(x)IncompleteGamma(x^2,x+1)").unwrap();
        let incomplete_result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &incomplete).unwrap();
        let incomplete_source = incomplete_result.value().unwrap().print_source();
        assert!(incomplete_source.contains("Ln(x^2)"), "{incomplete_source}");
        assert!(
            incomplete_source.contains("Integrate("),
            "{incomplete_source}"
        );
        assert!(incomplete_source.contains("Ln("), "{incomplete_source}");
        assert!(!incomplete_source.contains("D("), "{incomplete_source}");
        assert!(incomplete_result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| {
                event.bindings.iter().any(|(key, value)| {
                    key == "derivative_rule" && value == "function.incomplete-gamma.derivative"
                })
            }));
    }

    #[test]
    fn incomplete_gamma_parameter_derivative_uses_a_capture_free_formal_integral() {
        let mut engine = RustEngine::spawn().unwrap();
        let expression = crate::elaboration::elaborate(
            "D(x)IncompleteGamma(YaClueIncompleteGammaDerivativeInternal0T,x)",
        )
        .unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &expression).unwrap();
        let source = result.value().unwrap().print_source();
        assert!(source.contains("Integrate(s,0,"), "{source}");
        assert!(!source.contains("YaClueIncompleteGammaDerivativeInternal1T"));
        assert!(!source.contains("D("));
    }

    #[test]
    fn bessel_j_uses_adjacent_orders_and_preserves_the_order_role() {
        let mut engine = RustEngine::spawn().unwrap();
        let expression = crate::elaboration::elaborate("D(x)BesselJ(n,Sin(x^2))").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &expression).unwrap();
        let source = result.value().unwrap().print_source();
        assert!(source.contains("BesselJ(n-1,Sin(x^2))"), "{source}");
        assert!(source.contains("BesselJ(n+1,Sin(x^2))"), "{source}");
        assert!(source.contains("Cos(x^2)"), "{source}");
        assert!(source.ends_with("*x"), "{source}");
        assert!(!source.contains("D("), "{source}");
        assert!(result.trace.as_ref().unwrap().events.iter().any(|event| {
            event
                .bindings
                .iter()
                .any(|(key, value)| key == "parameter_roles" && value == "order,argument")
        }));

        let half_order = crate::elaboration::elaborate("D(x)BesselJ(1/2,x)").unwrap();
        let half_order_result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &half_order).unwrap();
        let half_order_source = half_order_result.value().unwrap().print_source();
        assert!(half_order_source.matches("BesselJ(").count() == 2);
        assert!(!half_order_source.contains("Sin("), "{half_order_source}");

        let varying_order = crate::elaboration::elaborate("D(x)BesselJ(x,x^2)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &varying_order).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .contains("BesselJ(x,x^2)"));
    }

    #[test]
    fn registered_function_descriptors_have_one_role_per_parameter_slot() {
        for rule in FUNCTION_DERIVATIVE_RULES {
            assert_eq!(rule.parameter_roles.len(), rule.arity, "{}", rule.id);
        }
    }

    #[test]
    fn held_integral_degrades_to_conditional_under_integral_derivative() {
        let mut engine = RustEngine::spawn().unwrap();
        let expression =
            crate::elaboration::elaborate("D(x)(Integrate(t,0,1)(Sin(x*t)/(1+t^2)))").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &expression).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(matches!(
            result.subject().unwrap().semantics.interpretation,
            SemanticInterpretation::HeldTypedApplication(ref application)
                if application.operator == OperatorId::Integral
        ));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .contains("Integrate"));
        assert!(result
            .subject()
            .unwrap()
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .any(|condition| matches!(condition, Condition::Unknown { .. })));
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| { event.rule == "differentiate-under-integral-sign" }));
    }

    #[test]
    fn unregistered_functions_do_not_create_implicit_derivative_rules() {
        let mut engine = RustEngine::spawn().unwrap();
        let expression = crate::elaboration::elaborate("D(x)UnknownSpecial(x)").unwrap();
        let result =
            crate::arithmetic::execute_elaborated_structure(&mut engine, &expression).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|event| { event.rule != "derivative-registered-function-chain-rule" }));
    }
}
