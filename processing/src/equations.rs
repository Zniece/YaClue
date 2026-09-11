//! Structured API for algebraic equations and systems.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, direct_function_equation, fresh_internal_symbols, root_call,
    strip_tex_delimiters, validate_expression, validate_symbol,
};
use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, CapabilitySet, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta, OperatorId, RuleEvent,
    RuleImportance, RulePayload, RulePresentation, RuleTrace, SemanticInterpretation,
    SemanticOperation, SemanticState,
};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus {
    Solved,
    NoSolution,
    Infinite,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableSource {
    Explicit,
    Inferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveCompleteness {
    Complete,
    Parametric,
    Periodic,
    Representative,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterDomain {
    Integers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SolutionParameter {
    pub symbol: String,
    pub domain: ParameterDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SolutionFamily {
    pub assignments: Vec<Assignment>,
    pub parameters: Vec<SolutionParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assignment {
    pub variable: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SolveResult {
    pub status: SolveStatus,
    /// Alternative solution sets. Each inner vector is one simultaneous solution.
    pub solutions: Vec<Vec<Assignment>>,
    pub raw: String,
    pub tex: String,
    /// Variables actually passed to Solve.
    pub variables: Vec<String>,
    pub variable_source: VariableSource,
    /// Symbols left in solved values, interpreted as parameters.
    pub parameters: Vec<String>,
    pub completeness: SolveCompleteness,
    /// Complete structured families when the solution requires bound integer
    /// parameters. `solutions` remains the finite representative-root view.
    pub families: Vec<SolutionFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolveRequest {
    pub equations: Vec<String>,
    pub variables: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SolveOperation;

impl SemanticOperation<SolveRequest> for SolveOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &SolveRequest,
    ) -> Result<Computation, EngineError> {
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::SolveEquation)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备方程求解能力".into(),
            ));
        }
        let equation_refs = request
            .equations
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let variable_refs = request
            .variables
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let result = solve(engine, &equation_refs, &variable_refs)?;
        let output_source = match result.status {
            SolveStatus::Solved | SolveStatus::NoSolution => result.raw.clone(),
            SolveStatus::Infinite => format!("AllSolutions({{{}}})", result.variables.join(",")),
            SolveStatus::Unresolved => format!(
                "Solve({{{}}},{{{}}})",
                request.equations.join(","),
                result.variables.join(",")
            ),
        };
        let unresolved = result.status == SolveStatus::Unresolved;
        let no_value = result.status == SolveStatus::NoSolution;
        let semantics = SemanticState {
            kind: if unresolved || no_value {
                ValueKind::Unevaluated
            } else {
                ValueKind::SolutionSet
            },
            interpretation: if unresolved {
                SemanticInterpretation::HeldApplication {
                    operator: "Solve".into(),
                }
            } else if no_value {
                SemanticInterpretation::StructuredUnevaluated {
                    reason: "equation system has no solution".into(),
                }
            } else {
                SemanticInterpretation::SolutionSet {
                    variables: result.variables.clone(),
                    parameters: result.parameters.clone(),
                }
            },
            metadata: if unresolved {
                ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
            } else if no_value {
                ResultMetadata::no_result(Exactness::Symbolic, OutcomeReason::MathematicalAbsence)
            } else {
                ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
            },
            capabilities: CapabilitySet::empty(),
            requirements: Vec::new(),
        };
        let parsed = object_from_source(input.id, &output_source, semantics.clone())?;
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(semantics),
            overlay: None,
            normalization: (!unresolved && !no_value).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::Solve),
            }),
        });
        let event = RuleEvent {
            rule: match result.status {
                SolveStatus::Solved => "solve-equations",
                SolveStatus::NoSolution => "solve-no-solution",
                SolveStatus::Infinite => "solve-all-values",
                SolveStatus::Unresolved => "hold-solve",
            }
            .into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: vec![(
                "variables".into(),
                format!("{{{}}}", result.variables.join(",")),
            )],
            conditions: Vec::new(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: (!unresolved).then(|| RulePresentation {
                expression: output.print_source(),
                explanation: match result.status {
                    SolveStatus::Solved => "求得并验证方程解集。",
                    SolveStatus::NoSolution => "方程组没有解。",
                    SolveStatus::Infinite => "方程对指定变量恒成立。",
                    SolveStatus::Unresolved => unreachable!(),
                }
                .into(),
                tex_override: Some(result.tex),
            }),
        };
        Ok(Computation {
            output: if unresolved {
                ComputationOutput::Held(output)
            } else if no_value {
                ComputationOutput::NoValue(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EquationStepResult {
    pub result: SolveResult,
    pub steps: Vec<Step>,
}

pub fn solve_steps(
    engine: &mut dyn Engine,
    equation: &str,
    variable: &str,
) -> Result<EquationStepResult, EngineError> {
    solve_steps_with_verbosity(engine, equation, variable, StepVerbosity::Detailed)
}

pub fn solve_steps_with_verbosity(
    engine: &mut dyn Engine,
    equation: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<EquationStepResult, EngineError> {
    let result = solve(engine, &[equation], &[variable])?;
    let steps = algebraic_equation_steps(engine, equation, variable, &result, verbosity)?;
    Ok(EquationStepResult { result, steps })
}

pub fn solve_system_steps(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<EquationStepResult, EngineError> {
    solve_system_steps_with_verbosity(engine, equations, variables, StepVerbosity::Detailed)
}

pub fn solve_system_steps_with_verbosity(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
    verbosity: StepVerbosity,
) -> Result<EquationStepResult, EngineError> {
    let result = solve(engine, equations, variables)?;
    let normalized = equations
        .iter()
        .map(|equation| {
            equation
                .split_once("==")
                .map(|(left, right)| format!("({left})-({right})==0"))
                .unwrap_or_else(|| format!("{equation}==0"))
        })
        .collect::<Vec<_>>();
    let mut events = vec![StepEvent::new(
        "equation-system-start",
        &format!("{{{}}}", equations.join(",")),
        "建立联立方程组。",
        StepImportance::Routine,
    )];
    events.push(StepEvent::new(
        "equation-system-variables",
        &format!("{{{}}}", result.variables.join(",")),
        match result.variable_source {
            VariableSource::Explicit => "使用调用方指定的未知量。",
            VariableSource::Inferred => "从全部方程中发现未知量并固定求解顺序。",
        },
        StepImportance::Normal,
    ));
    events.push(StepEvent::new(
        "equation-system-normalize",
        &format!("{{{}}}", normalized.join(",")),
        "将每个方程移到一边，形成同一个消元系统。",
        StepImportance::Normal,
    ));
    if equations.len() > 1 || result.variables.len() > 1 {
        events.push(StepEvent::new(
            "equation-system-eliminate",
            &result.raw,
            "联立消元并保留相容的解分支。",
            StepImportance::Key,
        ));
    }
    match result.status {
        SolveStatus::Solved => {
            for solution in &result.solutions {
                events.push(StepEvent::new(
                    "equation-system-branch",
                    &assignments_expression(solution),
                    if result.completeness == SolveCompleteness::Parametric {
                        "得到一个参数化解分支；未被消去的符号作为自由参数。"
                    } else {
                        "得到一个同时满足全部方程的解分支。"
                    },
                    StepImportance::Normal,
                ));
            }
            events.push(StepEvent::new(
                "equation-system-verify",
                "0",
                "将各分支代回全部原方程，残差均为零。",
                StepImportance::Routine,
            ));
        }
        SolveStatus::NoSolution => events.push(StepEvent::new(
            "equation-system-inconsistent",
            "False",
            "消元后不存在能同时满足全部方程的分支。",
            StepImportance::Key,
        )),
        SolveStatus::Infinite | SolveStatus::Unresolved => {}
    }
    events.push(StepEvent::new(
        "equation-system-result",
        &result.raw,
        match result.status {
            SolveStatus::Solved if result.completeness == SolveCompleteness::Parametric => {
                "得到参数化解集。"
            }
            SolveStatus::Solved => "得到经过原方程验证的解集。",
            SolveStatus::NoSolution => "方程组无解。",
            SolveStatus::Infinite => "方程组恒成立，解不唯一。",
            SolveStatus::Unresolved => "当前解析方法未能完整求解该方程组。",
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    Ok(EquationStepResult { result, steps })
}

fn algebraic_equation_steps(
    engine: &mut dyn Engine,
    equation: &str,
    variable: &str,
    result: &SolveResult,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let residual = equation
        .split_once("==")
        .map(|(left, right)| format!("({left})-({right})"))
        .unwrap_or_else(|| equation.to_string());
    let verification_checks = result
        .solutions
        .iter()
        .filter(|solution| solution.len() == 1)
        .map(|solution| {
            let assignment = &solution[0];
            format!(
                "IsZero(Simplify(Eval(ApplyPure(\"Subst\",{{{},{},{residual}}}))))",
                assignment.variable, assignment.value
            )
        })
        .collect::<Vec<_>>();
    let checks = verification_checks.join(",");
    let denominator = equation
        .split_once("==")
        .map(|(left, right)| {
            format!("NormalForm(GetNumerDenom({left})[2]*GetNumerDenom({right})[2])")
        })
        .unwrap_or_else(|| format!("GetNumerDenom({equation})[2]"));
    let [p, d, f, q] = fresh_internal_symbols(
        "EquationDiagnostic",
        &[equation, variable, &residual, &denominator, &checks],
        ["Polynomial", "Degree", "Factored", "Denominator"],
    );
    let diagnostic = engine.eval_expr(&format!(
        "[Local({p},{d},{f},{q}); {p}:=NormalForm({residual}); {q}:={denominator}; If(CanBeUni({variable},{p}), [{d}:=Degree({p},{variable}); {f}:=If({d}<=8,Factor({p}),{p}); {{True,{p},{d},{f},{{{checks}}},{q}}};], {{False,{p},0,{p},{{{checks}}},{q}}});]"
    ))?;
    let Expr::Call { head, args } = diagnostic else {
        return Err(EngineError::Parse("方程步骤诊断不是列表".into()));
    };
    if head != "List" || args.len() != 6 {
        return Err(EngineError::Parse("方程步骤诊断形态异常".into()));
    }
    let polynomial = matches!(&args[0], Expr::Symbol(value) if value == "True");
    let normalized = args[1].to_string();
    let degree = match &args[2] {
        Expr::Number(value) => value.parse::<usize>().ok(),
        _ => None,
    };
    let factored = args[3].to_string();
    let denominator = args[5].to_string();
    let has_domain_exclusion = analyze_expression(&denominator, "方程分母")?
        .symbols
        .iter()
        .any(|symbol| symbol == variable);
    let verified = match &args[4] {
        Expr::Call { head, args } if head == "List" => args
            .iter()
            .map(|value| matches!(value, Expr::Symbol(symbol) if symbol == "True"))
            .collect::<Vec<_>>(),
        _ => return Err(EngineError::Parse("候选解残差证书不是列表".into())),
    };

    let mut events = vec![StepEvent::new(
        "equation-start",
        equation,
        &format!("建立关于 {variable} 的方程。"),
        StepImportance::Routine,
    )];
    if has_domain_exclusion {
        events.push(StepEvent::new(
            "equation-domain-exclusion",
            &format!("{denominator}!=0"),
            "原方程的分母不能为零；这些值不属于定义域。",
            StepImportance::Key,
        ));
    }
    if polynomial {
        events.push(StepEvent::new(
            "equation-normalize",
            &format!("{normalized}==0"),
            "将方程移到一边并整理为多项式标准形。",
            StepImportance::Normal,
        ));
        match degree {
            Some(1) => events.push(StepEvent {
                rule: "equation-linear".into(),
                expr: result.raw.clone(),
                why: format!("合并同类项并解出 {variable}。"),
                importance: StepImportance::Normal,
            }),
            Some(2) => events.push(StepEvent {
                rule: "equation-quadratic".into(),
                expr: format!("{normalized}==0"),
                why: "使用二次方程求根公式求出各个分支。".into(),
                importance: StepImportance::Normal,
            }),
            Some(3..=8) if factored != normalized => events.push(StepEvent {
                rule: "equation-factor".into(),
                expr: format!("{factored}==0"),
                why: "因式分解后令每个因式分别为零。".into(),
                importance: StepImportance::Key,
            }),
            _ => {}
        }
    } else if let Some(event) = direct_radical_event(equation, variable)? {
        events.push(event);
    } else if let Some(event) = direct_inverse_event(equation, variable)? {
        events.push(event);
    }
    if result.status == SolveStatus::Solved {
        let mut certificate = 0;
        for solution in &result.solutions {
            if solution.len() == 1 {
                if !verified.get(certificate).copied().unwrap_or(false) {
                    return Err(EngineError::Parse(format!(
                        "Solve 返回的候选解未通过原方程残差检查: {}=={}",
                        solution[0].variable, solution[0].value
                    )));
                }
                certificate += 1;
                events.push(StepEvent {
                    rule: "equation-branch".into(),
                    expr: format!("{}=={}", solution[0].variable, solution[0].value),
                    why: "得到一个候选解分支。".into(),
                    importance: StepImportance::Normal,
                });
                events.push(StepEvent {
                    rule: "equation-verify".into(),
                    expr: "0".into(),
                    why: if has_domain_exclusion {
                        "候选值不在分母排除集中，代回原方程后的残差为零。".into()
                    } else {
                        "将该候选解代回原方程，残差为零；去根号产生的增根会在此被拒绝。".into()
                    },
                    importance: StepImportance::Routine,
                });
            }
        }
    }
    events.push(StepEvent {
        rule: "equation-result".into(),
        expr: result.raw.clone(),
        why: match result.status {
            SolveStatus::Solved => "得到方程的解集。",
            SolveStatus::NoSolution => "方程没有满足条件的解。",
            SolveStatus::Infinite => "方程对该变量恒成立。",
            SolveStatus::Unresolved => "当前解析方法未能求解该方程。",
        }
        .into(),
        importance: StepImportance::Key,
    });

    render_events(engine, events, verbosity)
}

fn direct_radical_event(equation: &str, variable: &str) -> Result<Option<StepEvent>, EngineError> {
    let Some((left, right)) = equation.split_once("==") else {
        return Ok(None);
    };
    for (radical, other) in [(left.trim(), right.trim()), (right.trim(), left.trim())] {
        let Some(call) = root_call(radical, "根式方程")? else {
            continue;
        };
        if call.head != "Sqrt" || call.arguments.len() != 1 {
            continue;
        }
        if analyze_expression(other, "根式方程另一侧")?
            .symbols
            .iter()
            .any(|symbol| symbol == variable)
        {
            continue;
        }
        return Ok(Some(StepEvent::new(
            "equation-radical-square",
            &format!("{}==({other})^2", call.arguments[0]),
            "孤立平方根后两边平方；这一步可能产生增根，最终必须代回原方程。",
            StepImportance::Key,
        )));
    }
    Ok(None)
}

fn assignments_expression(assignments: &[Assignment]) -> String {
    format!(
        "{{{}}}",
        assignments
            .iter()
            .map(|assignment| format!("{}=={}", assignment.variable, assignment.value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn direct_inverse_event(equation: &str, variable: &str) -> Result<Option<StepEvent>, EngineError> {
    let Some(function) = direct_function_equation(equation, variable, &["Exp", "Ln"])? else {
        return Ok(None);
    };
    let Some((left, right)) = equation.split_once("==") else {
        return Ok(None);
    };
    let other = if analyze_expression(left, "方程左侧")?
        .symbols
        .iter()
        .any(|symbol| symbol == variable)
    {
        right.trim()
    } else {
        left.trim()
    };
    let event = match function.as_str() {
        "Exp" => StepEvent::new(
            "equation-invert-exponential",
            &format!("{variable}==Ln({other})"),
            "两边取自然对数，利用自然对数与指数函数互为反函数。",
            StepImportance::Key,
        ),
        "Ln" => StepEvent::new(
            "equation-invert-logarithm",
            &format!("{variable}==Exp({other})"),
            "两边取指数，利用指数函数与自然对数互为反函数。",
            StepImportance::Key,
        ),
        _ => return Ok(None),
    };
    Ok(Some(event))
}

pub fn solve(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<SolveResult, EngineError> {
    solve_configured(engine, equations, variables, true)
}

pub(crate) fn solve_without_residual_verification(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<SolveResult, EngineError> {
    solve_configured(engine, equations, variables, false)
}

fn solve_configured(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
    verify_residuals: bool,
) -> Result<SolveResult, EngineError> {
    if equations.is_empty() {
        return Err(EngineError::InvalidInput("至少需要一个方程".into()));
    }
    for equation in equations {
        validate_expression(equation, "方程")?;
    }
    let variable_source = if variables.is_empty() {
        VariableSource::Inferred
    } else {
        VariableSource::Explicit
    };
    let inferred;
    let variables = if variables.is_empty() {
        inferred = infer_variables(equations)?;
        if inferred.is_empty() {
            return Err(EngineError::InvalidInput("方程中没有可求解变量".into()));
        }
        inferred.iter().map(String::as_str).collect::<Vec<_>>()
    } else {
        variables.to_vec()
    };
    for variable in &variables {
        validate_symbol(variable, "求解变量")?;
    }
    let mut unique = variables.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != variables.len() {
        return Err(EngineError::InvalidInput("求解变量不能重复".into()));
    }

    let rectangular_system = if equations.len() > variables.len() {
        infer_variables(equations)?
            .iter()
            .all(|symbol| variables.iter().any(|variable| *variable == symbol))
    } else {
        equations.len() < variables.len()
    };
    if rectangular_system {
        return solve_rectangular(
            engine,
            equations,
            &variables,
            variable_source,
            verify_residuals,
        );
    }

    let scalar = equations.len() == 1 && variables.len() == 1;
    let solve_call = if scalar {
        format!("Solve({}, {})", equations[0], variables[0])
    } else {
        format!(
            "Solve({{{}}}, {{{}}})",
            equations.join(","),
            variables.join(",")
        )
    };
    // Solve returns {} both for valid no-solution cases and unsupported
    // equations. Capture its global status flags in the same request.
    let [solution, failed, type_error] = fresh_internal_symbols(
        "SolveWrapper",
        &[&solve_call],
        ["Solution", "Failed", "TypeError"],
    );
    let command = format!(
        "[ClearErrors(); Local({solution},{failed},{type_error}); {solution}:={solve_call}; {failed}:=IsError(\"Solve'Fails\"); {type_error}:=IsError(\"Solve'TypeError\"); ClearErrors(); {{{solution},{failed},{type_error}}};]"
    );
    let wrapper = engine.eval_expr(&command)?;
    let (raw_expr, failed, type_error) = parse_wrapper(wrapper)?;
    if type_error {
        return Err(EngineError::InvalidInput(
            "Solve 拒绝了求解变量或参数类型".into(),
        ));
    }
    let raw = raw_expr.to_string();
    if failed {
        let tex = engine
            .eval(&raw)
            .map(|result| strip_tex_delimiters(&result.tex))
            .unwrap_or_else(|_| raw.clone());
        return Ok(SolveResult {
            status: SolveStatus::Unresolved,
            solutions: vec![],
            raw,
            tex,
            variables: variables.iter().map(|value| (*value).to_string()).collect(),
            variable_source,
            parameters: vec![],
            completeness: SolveCompleteness::Unknown,
            families: vec![],
        });
    }

    let parsed_solutions = parse_solutions(&raw_expr, scalar)?;
    let had_candidates = !parsed_solutions.is_empty();
    let (mut solutions, rejected_candidates) = if verify_residuals {
        verify_candidates(engine, parsed_solutions, equations)?
    } else {
        (parsed_solutions, false)
    };
    let infinite = solutions.iter().flatten().any(|assignment| {
        assignment.variable == assignment.value
            && variables
                .iter()
                .any(|variable| *variable == assignment.variable)
    });
    if infinite {
        solutions.clear();
    }
    let status = if infinite {
        SolveStatus::Infinite
    } else if solutions.is_empty() {
        if had_candidates && rejected_candidates {
            SolveStatus::Unresolved
        } else {
            SolveStatus::NoSolution
        }
    } else {
        SolveStatus::Solved
    };
    let mut parameters = Vec::new();
    for assignment in solutions.iter().flatten() {
        parameters.extend(
            infer_variables(&[assignment.value.as_str()])?
                .into_iter()
                .filter(|symbol| !variables.iter().any(|variable| *variable == symbol)),
        );
    }
    parameters.sort();
    parameters.dedup();
    let mut completeness = if rejected_candidates {
        SolveCompleteness::Unknown
    } else if parameters.is_empty() {
        SolveCompleteness::Complete
    } else {
        SolveCompleteness::Parametric
    };
    let mut tex = String::new();
    let mut families = Vec::new();
    let has_trigonometric_function = equations.iter().try_fold(false, |found, equation| {
        let analysis = analyze_expression(equation, "方程")?;
        Ok::<_, EngineError>(
            found
                || analysis
                    .function_heads
                    .iter()
                    .any(|head| matches!(head.as_str(), "Sin" | "Cos" | "Tan")),
        )
    })?;
    if status == SolveStatus::Solved && has_trigonometric_function {
        completeness = SolveCompleteness::Representative;
        if scalar && parameters.is_empty() {
            if let Some(function) =
                direct_function_equation(equations[0], variables[0], &["Sin", "Cos", "Tan"])?
            {
                let parameter = unused_integer_parameter(equations, &variables)?;
                families = periodic_families(&solutions, &function, &parameter);
                if !families.is_empty() {
                    let display = families
                        .iter()
                        .flat_map(|family| family.assignments.iter())
                        .map(|assignment| format!("{}=={}", assignment.variable, assignment.value))
                        .collect::<Vec<_>>()
                        .join(",");
                    tex = strip_tex_delimiters(&engine.eval(&format!("{{{display}}}"))?.tex);
                    completeness = SolveCompleteness::Periodic;
                }
            }
        }
    }
    if tex.is_empty() {
        tex = engine
            .eval(&raw)
            .map(|result| strip_tex_delimiters(&result.tex))
            .unwrap_or_else(|_| raw.clone());
    }
    Ok(SolveResult {
        status,
        solutions,
        raw,
        tex,
        variables: variables.iter().map(|value| (*value).to_string()).collect(),
        variable_source,
        parameters,
        completeness,
        families,
    })
}

/// Solve a rectangular system through bounded square subproblems. Candidates
/// are accepted only after all original equations agree, so this path can use
/// the mature `Solve` implementation without maintaining another eliminator.
fn solve_rectangular(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
    variable_source: VariableSource,
    verify_residuals: bool,
) -> Result<SolveResult, EngineError> {
    const MAX_SUBPROBLEMS: usize = 32;
    let underdetermined = equations.len() < variables.len();
    let search_truncated = if underdetermined {
        combinations_exceed(variables.len(), equations.len(), MAX_SUBPROBLEMS)
    } else {
        combinations_exceed(equations.len(), variables.len(), MAX_SUBPROBLEMS)
    };
    let subproblems: Vec<(Vec<&str>, Vec<&str>)> = if underdetermined {
        combinations(variables, equations.len(), MAX_SUBPROBLEMS)
            .into_iter()
            .map(|subset| (equations.to_vec(), subset))
            .collect()
    } else {
        combinations(equations, variables.len(), MAX_SUBPROBLEMS)
            .into_iter()
            .map(|subset| (subset, variables.to_vec()))
            .collect()
    };

    let mut saw_complete_subproblem = false;
    let mut saw_unresolved = false;
    for (equation_subset, variable_subset) in subproblems {
        let mut result =
            solve_configured(engine, &equation_subset, &variable_subset, verify_residuals)?;
        match result.status {
            SolveStatus::Solved => {
                saw_complete_subproblem |= result.completeness == SolveCompleteness::Complete;
                let (verified, rejected) = verify_candidates(engine, result.solutions, equations)?;
                result.solutions = verified;
                if result.solutions.is_empty() {
                    saw_unresolved |= rejected;
                    continue;
                }
                result.status = SolveStatus::Solved;
                result.variables = variables.iter().map(|value| (*value).to_string()).collect();
                result.variable_source = variable_source;
                if underdetermined {
                    result.parameters = rectangular_parameters(&result.solutions)?;
                    result.completeness = SolveCompleteness::Parametric;
                }
                return Ok(result);
            }
            SolveStatus::Unresolved => saw_unresolved = true,
            SolveStatus::NoSolution | SolveStatus::Infinite => {
                saw_complete_subproblem = true;
            }
        }
    }

    let unresolved = search_truncated || (saw_unresolved && !saw_complete_subproblem);
    Ok(SolveResult {
        status: if unresolved {
            SolveStatus::Unresolved
        } else {
            SolveStatus::NoSolution
        },
        solutions: Vec::new(),
        raw: "List()".into(),
        tex: "\\left( \\right) ".into(),
        variables: variables.iter().map(|value| (*value).to_string()).collect(),
        variable_source,
        parameters: Vec::new(),
        completeness: if unresolved {
            SolveCompleteness::Unknown
        } else {
            SolveCompleteness::Complete
        },
        families: Vec::new(),
    })
}

fn unused_integer_parameter(equations: &[&str], variables: &[&str]) -> Result<String, EngineError> {
    let mut used = infer_variables(equations)?;
    used.extend(variables.iter().map(|value| (*value).to_string()));
    for index in 0usize.. {
        let candidate = if index == 0 {
            "k".to_string()
        } else {
            format!("k{index}")
        };
        if !used.contains(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!("an unused indexed parameter always exists")
}

fn periodic_families(
    representatives: &[Vec<Assignment>],
    function: &str,
    parameter: &str,
) -> Vec<SolutionFamily> {
    let period = if function == "Tan" { "Pi" } else { "2*Pi" };
    representatives
        .iter()
        .filter(|solution| solution.len() == 1)
        .map(|solution| SolutionFamily {
            assignments: vec![Assignment {
                variable: solution[0].variable.clone(),
                value: format!("({})+({period}*{parameter})", solution[0].value),
            }],
            parameters: vec![SolutionParameter {
                symbol: parameter.to_string(),
                domain: ParameterDomain::Integers,
            }],
        })
        .collect()
}

fn combinations<T: Copy>(items: &[T], choose: usize, limit: usize) -> Vec<Vec<T>> {
    fn visit<T: Copy>(
        items: &[T],
        choose: usize,
        start: usize,
        current: &mut Vec<T>,
        output: &mut Vec<Vec<T>>,
        limit: usize,
    ) {
        if output.len() >= limit {
            return;
        }
        if current.len() == choose {
            output.push(current.clone());
            return;
        }
        let needed = choose - current.len();
        for index in start..=items.len().saturating_sub(needed) {
            current.push(items[index]);
            visit(items, choose, index + 1, current, output, limit);
            current.pop();
            if output.len() >= limit {
                break;
            }
        }
    }

    if choose == 0 || choose > items.len() {
        return Vec::new();
    }
    let mut output = Vec::new();
    visit(items, choose, 0, &mut Vec::new(), &mut output, limit);
    output
}

fn combinations_exceed(items: usize, choose: usize, limit: usize) -> bool {
    let choose = choose.min(items.saturating_sub(choose));
    let mut count = 1usize;
    for index in 0..choose {
        let Some(next) = count
            .checked_mul(items - index)
            .map(|value| value / (index + 1))
        else {
            return true;
        };
        count = next;
        if count > limit {
            return true;
        }
    }
    false
}

fn verify_candidates(
    engine: &mut dyn Engine,
    solutions: Vec<Vec<Assignment>>,
    equations: &[&str],
) -> Result<(Vec<Vec<Assignment>>, bool), EngineError> {
    if solutions.is_empty() {
        return Ok((solutions, false));
    }
    let batches = solutions
        .iter()
        .map(|solution| {
            let checks = equations.iter().map(|equation| {
                let mut substituted = equation
                    .split_once("==")
                    .map(|(left, right)| format!("({left})-({right})"))
                    .unwrap_or_else(|| (*equation).to_string());
                for assignment in solution {
                    substituted = format!(
                        "Subst({},{})({substituted})",
                        assignment.variable, assignment.value
                    );
                }
                let [residual] =
                    fresh_internal_symbols("EquationResidual", &[&substituted], ["Value"]);
                format!(
                    "[Local({residual}); {residual}:=Simplify({substituted}); \
                     If(IsNumber({residual}),{residual},N({residual},30));]"
                )
            });
            format!("{{{}}}", checks.collect::<Vec<_>>().join(","))
        })
        .collect::<Vec<_>>();
    let result = engine.eval_expr(&format!("{{{}}}", batches.join(",")))?;
    let verdicts = list_items(&result)?;
    if verdicts.len() != solutions.len() {
        return Err(EngineError::Parse(
            "方程候选验证结果数量与候选数量不一致".into(),
        ));
    }
    let mut verified = Vec::with_capacity(solutions.len());
    for (solution, verdict) in solutions.into_iter().zip(verdicts) {
        let checks = list_items(verdict)?;
        if checks.len() != equations.len() {
            return Err(EngineError::Parse(
                "方程候选验证结果数量与方程数量不一致".into(),
            ));
        }
        if checks.iter().all(residual_is_zero) {
            verified.push(solution);
        }
    }
    let rejected = verified.len() != verdicts.len();
    Ok((verified, rejected))
}

fn residual_is_zero(result: &Expr) -> bool {
    matches!(result, Expr::Number(value) if value.parse::<f64>().is_ok_and(|number| number.is_finite() && number.abs() <= 1e-20))
}

fn rectangular_parameters(solutions: &[Vec<Assignment>]) -> Result<Vec<String>, EngineError> {
    let assigned: Vec<_> = solutions
        .iter()
        .flatten()
        .map(|assignment| assignment.variable.as_str())
        .collect();
    let mut parameters = Vec::new();
    for assignment in solutions.iter().flatten() {
        parameters.extend(
            infer_variables(&[assignment.value.as_str()])?
                .into_iter()
                .filter(|symbol| !assigned.iter().any(|variable| *variable == symbol)),
        );
    }
    parameters.sort();
    parameters.dedup();
    Ok(parameters)
}

fn infer_variables(expressions: &[&str]) -> Result<Vec<String>, EngineError> {
    let mut symbols = Vec::new();
    for expression in expressions {
        symbols.extend(analyze_expression(expression, "表达式")?.symbols);
    }
    symbols.sort();
    symbols.dedup();
    Ok(symbols)
}

fn parse_wrapper(wrapper: Expr) -> Result<(Expr, bool, bool), EngineError> {
    match wrapper {
        Expr::Call { head, mut args } if head == "List" && args.len() == 3 => {
            let type_error = bool_expr(args.pop().unwrap())?;
            let failed = bool_expr(args.pop().unwrap())?;
            Ok((args.pop().unwrap(), failed, type_error))
        }
        other => Err(EngineError::Parse(format!(
            "Solve 包装结果形态异常: {other}"
        ))),
    }
}

fn bool_expr(expr: Expr) -> Result<bool, EngineError> {
    match expr {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        other => Err(EngineError::Parse(format!(
            "Solve 状态标志不是布尔值: {other}"
        ))),
    }
}

fn parse_solutions(expr: &Expr, scalar: bool) -> Result<Vec<Vec<Assignment>>, EngineError> {
    let items = list_items(expr)?;
    if scalar {
        items
            .iter()
            .filter(|item| expression_is_finite(item))
            .map(|item| Ok(vec![parse_assignment(item)?]))
            .collect()
    } else {
        items
            .iter()
            .filter(|item| expression_is_finite(item))
            .map(|solution| list_items(solution)?.iter().map(parse_assignment).collect())
            .collect()
    }
}

fn expression_is_finite(expr: &Expr) -> bool {
    match expr {
        Expr::Symbol(value) => !matches!(value.as_str(), "Infinity" | "Undefined"),
        Expr::Call { args, .. } => args.iter().all(expression_is_finite),
        Expr::Number(_) => true,
    }
}

fn list_items(expr: &Expr) -> Result<&[Expr], EngineError> {
    match expr {
        Expr::Call { head, args } if head == "List" => Ok(args),
        other => Err(EngineError::Parse(format!("Solve 结果不是列表: {other}"))),
    }
}

fn parse_assignment(expr: &Expr) -> Result<Assignment, EngineError> {
    match expr {
        Expr::Call { head, args } if (head == "=" || head == "==") && args.len() == 2 => {
            Ok(Assignment {
                variable: args[0].to_string(),
                value: args[1].to_string(),
            })
        }
        other => Err(EngineError::Parse(format!("Solve 解不是等式: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::test_support::CountingEngine;

    #[test]
    fn solves_single_equations_and_parameterized_linear_forms() {
        let mut engine = RustEngine::spawn().unwrap();
        let quadratic = solve(&mut engine, &["x^2-3*x+2==0"], &["x"]).unwrap();
        assert_eq!(quadratic.status, SolveStatus::Solved);
        assert_eq!(quadratic.solutions.len(), 2);
        let mut roots: Vec<&str> = quadratic
            .solutions
            .iter()
            .map(|set| set[0].value.as_str())
            .collect();
        roots.sort_unstable();
        assert_eq!(roots, ["1", "2"]);

        let parameterized = solve(&mut engine, &["a+x*y==z"], &["x"]).unwrap();
        assert_eq!(parameterized.status, SolveStatus::Solved);
        let value = &parameterized.solutions[0][0].value;
        assert_eq!(
            engine
                .eval(&format!("Simplify(({value})-((z-a)/y))"))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn equation_results_and_steps_preserve_former_temporary_names() {
        let mut engine = RustEngine::spawn().unwrap();
        let stepped = solve_steps(&mut engine, "x+p==0", "x").unwrap();
        assert_eq!(stepped.result.status, SolveStatus::Solved);
        let value = &stepped.result.solutions[0][0].value;
        assert_eq!(
            engine
                .eval(&format!("Simplify(({value})+p)"))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
        assert!(stepped.steps.last().unwrap().expr.contains('p'));

        let result = solve(&mut engine, &["x==solution"], &["x"]).unwrap();
        assert_eq!(result.solutions[0][0].value, "solution");
    }

    #[test]
    fn infinity_substrings_are_ordinary_symbols() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = solve(&mut engine, &["x==MyInfinity", "y==1"], &["x", "y"]).unwrap();
        assert_eq!(result.status, SolveStatus::Solved);
        assert_eq!(result.solutions.len(), 1);
        assert!(result.solutions[0]
            .iter()
            .any(|assignment| assignment.variable == "x" && assignment.value == "MyInfinity"));
    }

    #[test]
    fn residual_verification_rejects_wrong_candidates() {
        let mut engine = RustEngine::spawn().unwrap();
        let candidates = vec![
            vec![Assignment {
                variable: "x".into(),
                value: "2".into(),
            }],
            vec![Assignment {
                variable: "x".into(),
                value: "3".into(),
            }],
        ];
        let (verified, rejected) = verify_candidates(&mut engine, candidates, &["x^2==4"]).unwrap();
        assert!(rejected);
        assert_eq!(verified.len(), 1);
        assert_eq!(verified[0][0].value, "2");
    }

    #[test]
    fn solves_systems_as_alternative_assignment_sets() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = solve(&mut engine, &["x*(y-1)==0", "y*(x-1)==0"], &["x", "y"]).unwrap();
        assert_eq!(result.status, SolveStatus::Solved);
        assert_eq!(result.solutions.len(), 2);
        assert!(result.solutions.iter().all(|solution| solution.len() == 2));
    }

    #[test]
    fn infers_system_variables_and_marks_explicit_parameters() {
        let mut engine = RustEngine::spawn().unwrap();
        let inferred = solve(&mut engine, &["x+y==3", "x-y==1"], &[]).unwrap();
        assert_eq!(inferred.status, SolveStatus::Solved);
        assert_eq!(inferred.variable_source, VariableSource::Inferred);
        assert_eq!(inferred.completeness, SolveCompleteness::Complete);
        assert_eq!(inferred.variables, ["x", "y"]);
        assert!(inferred.parameters.is_empty());
        assert_eq!(inferred.solutions.len(), 1);
        assert_eq!(inferred.solutions[0].len(), 2);

        let explicit_subset = solve(&mut engine, &["x+y==3", "x-y==1"], &["x"]).unwrap();
        assert_eq!(explicit_subset.status, SolveStatus::Unresolved);
        assert_eq!(explicit_subset.completeness, SolveCompleteness::Unknown);
        assert!(explicit_subset.solutions.is_empty());
        assert!(explicit_subset.parameters.is_empty());

        let parameterized = solve(&mut engine, &["a+x*y==z"], &["x"]).unwrap();
        assert_eq!(parameterized.variable_source, VariableSource::Explicit);
        assert_eq!(parameterized.completeness, SolveCompleteness::Parametric);
        assert_eq!(parameterized.parameters, ["a", "y", "z"]);
    }

    #[test]
    fn inference_excludes_function_names_constants_and_scientific_exponents() {
        assert_eq!(
            infer_variables(&["Sin(x)+Pi*y==1e10"]).unwrap(),
            vec!["x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn distinguishes_no_solution_infinite_and_unresolved() {
        let mut engine = RustEngine::spawn().unwrap();
        assert_eq!(
            solve(&mut engine, &["Sqrt(x)==-1"], &["x"]).unwrap().status,
            SolveStatus::NoSolution
        );
        assert_eq!(
            solve(&mut engine, &["0==0"], &["x"]).unwrap().status,
            SolveStatus::Infinite
        );
        assert_eq!(
            solve(&mut engine, &["x^x==1"], &["x"]).unwrap().status,
            SolveStatus::Unresolved
        );
    }

    #[test]
    fn rejects_invalid_requests_without_poisoning_the_engine() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(solve(&mut engine, &[], &["x"]).is_err());
        assert!(solve(&mut engine, &["x==1"], &["x", "x"]).is_err());
        assert!(solve(&mut engine, &["x);Echo(1);(x==1"], &["x"]).is_err());
        assert_eq!(
            solve(&mut engine, &["x==1"], &["x"]).unwrap().status,
            SolveStatus::Solved
        );
    }

    #[test]
    fn object_solve_distinguishes_solution_set_absence_infinite_and_held() {
        let object = |source: &str| {
            crate::semantic_core::object_from_source(
                crate::semantic_core::ObjectId(81),
                source,
                SemanticState {
                    kind: ValueKind::Equation,
                    interpretation: SemanticInterpretation::Equation,
                    metadata: ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty()),
                    capabilities: CapabilitySet::equation_input(),
                    requirements: Vec::new(),
                },
            )
            .unwrap()
        };
        let mut engine = RustEngine::spawn().unwrap();
        let compute = |engine: &mut RustEngine, equation: &str| {
            SolveOperation
                .compute(
                    engine,
                    &object(equation),
                    &SolveRequest {
                        equations: vec![equation.into()],
                        variables: vec!["x".into()],
                    },
                )
                .unwrap()
        };

        let solved = compute(&mut engine, "x^2==1");
        assert!(matches!(solved.output, ComputationOutput::Value(_)));
        assert_eq!(
            solved.value().unwrap().id,
            crate::semantic_core::ObjectId(81)
        );
        assert!(solved
            .value()
            .unwrap()
            .meets_normalization(NormalizationLevel::Domain));
        assert!(matches!(
            solved.value().unwrap().semantics.interpretation,
            SemanticInterpretation::SolutionSet { .. }
        ));

        assert!(matches!(
            compute(&mut engine, "Sqrt(x)==-1").output,
            ComputationOutput::NoValue(_)
        ));
        assert!(matches!(
            compute(&mut engine, "0==0").output,
            ComputationOutput::Value(_)
        ));
        let held = compute(&mut engine, "x^x==1");
        assert!(matches!(held.output, ComputationOutput::Held(_)));
        assert!(held.subject().unwrap().print_source().starts_with("Solve("));
    }

    #[test]
    fn handles_underdetermined_overdetermined_and_inconsistent_systems() {
        let mut engine = RustEngine::spawn().unwrap();

        let under = solve(&mut engine, &["x+y==3"], &["x", "y"]).unwrap();
        assert_eq!(under.status, SolveStatus::Solved);
        assert_eq!(under.completeness, SolveCompleteness::Parametric);
        assert_eq!(under.parameters, ["y"]);
        assert_eq!(under.solutions[0][0].variable, "x");

        let over = solve(&mut engine, &["x+y==3", "x-y==1", "x==2"], &["x", "y"]).unwrap();
        assert_eq!(over.status, SolveStatus::Solved);
        assert_eq!(over.solutions[0].len(), 2);

        let inconsistent = solve(&mut engine, &["x+y==3", "x+y==4"], &["x", "y"]).unwrap();
        assert_eq!(inconsistent.status, SolveStatus::NoSolution);
        assert!(inconsistent.solutions.is_empty());

        assert!(!combinations_exceed(3, 2, 32));
        assert!(combinations_exceed(20, 10, 32));
    }

    #[test]
    fn exposes_direct_trigonometric_roots_as_periodic_families() {
        let mut engine = RustEngine::spawn().unwrap();
        for (equation, expected_families, period) in [
            ("Sin(x)==0", 2, "2*Pi"),
            ("Cos(x)==0", 2, "2*Pi"),
            ("Tan(x)==1", 1, "Pi*k"),
        ] {
            let result = solve(&mut engine, &[equation], &["x"]).unwrap();
            assert_eq!(result.completeness, SolveCompleteness::Periodic);
            assert_eq!(result.families.len(), expected_families);
            assert!(result.families.iter().all(|family| {
                family.parameters[0].domain == ParameterDomain::Integers
                    && family.assignments[0].value.contains(period)
            }));
            for family in &result.families {
                for integer in [-2, 0, 3] {
                    let mut instantiated = family.assignments.clone();
                    instantiated.push(Assignment {
                        variable: family.parameters[0].symbol.clone(),
                        value: integer.to_string(),
                    });
                    let (verified, rejected) =
                        verify_candidates(&mut engine, vec![instantiated], &[equation]).unwrap();
                    assert!(!rejected && verified.len() == 1);
                }
            }
        }

        let composite = solve(&mut engine, &["Sin(2*x)==0"], &["x"]).unwrap();
        assert_eq!(composite.completeness, SolveCompleteness::Representative);
        assert!(composite.families.is_empty());
    }

    #[test]
    fn algebraic_steps_cover_linear_quadratic_and_factored_polynomials() {
        let mut engine = RustEngine::spawn().unwrap();

        let linear = solve_steps(&mut engine, "2*x+3==7", "x").unwrap();
        assert_eq!(linear.result.status, SolveStatus::Solved);
        assert!(linear
            .steps
            .iter()
            .any(|step| step.rule == "equation-linear"));
        assert!(linear
            .steps
            .iter()
            .any(|step| step.rule == "equation-verify"));

        let quadratic = solve_steps(&mut engine, "x^2-3*x+2==0", "x").unwrap();
        assert!(quadratic
            .steps
            .iter()
            .any(|step| step.rule == "equation-quadratic"));
        assert_eq!(
            quadratic
                .steps
                .iter()
                .filter(|step| step.rule == "equation-verify")
                .count(),
            2
        );

        let factored = solve_steps(&mut engine, "x^3-x==0", "x").unwrap();
        assert!(factored
            .steps
            .iter()
            .any(|step| step.rule == "equation-factor"));
    }

    #[test]
    fn algebraic_step_verbosity_filters_semantic_events() {
        let mut engine = RustEngine::spawn().unwrap();
        let detailed =
            solve_steps_with_verbosity(&mut engine, "x^2-3*x+2==0", "x", StepVerbosity::Detailed)
                .unwrap();
        let standard =
            solve_steps_with_verbosity(&mut engine, "x^2-3*x+2==0", "x", StepVerbosity::Standard)
                .unwrap();
        let concise =
            solve_steps_with_verbosity(&mut engine, "x^2-3*x+2==0", "x", StepVerbosity::Concise)
                .unwrap();
        assert!(detailed.steps.len() > standard.steps.len());
        assert!(standard.steps.len() > concise.steps.len());
        assert_eq!(concise.steps.len(), 1);
        assert_eq!(concise.steps[0].rule, "equation-result");
    }

    #[test]
    fn equation_steps_use_one_filtered_tex_batch() {
        let mut engine = CountingEngine::spawn();
        solve(&mut engine, &["x^2-3*x+2==0"], &["x"]).unwrap();
        assert!(engine.batch_sizes.is_empty());

        engine.reset_counts();
        let detailed =
            solve_steps_with_verbosity(&mut engine, "x^2-3*x+2==0", "x", StepVerbosity::Detailed)
                .unwrap();
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);

        engine.reset_counts();
        let concise =
            solve_steps_with_verbosity(&mut engine, "x^2-3*x+2==0", "x", StepVerbosity::Concise)
                .unwrap();
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
        assert!(concise.steps.len() < detailed.steps.len());
    }

    #[test]
    fn direct_exponential_and_logarithmic_steps_use_input_transformations() {
        let mut engine = RustEngine::spawn().unwrap();
        for (equation, rule, transformed) in [
            ("Exp(x)==2", "equation-invert-exponential", "x==Ln(2)"),
            ("1==Ln(x)", "equation-invert-logarithm", "x==Exp(1)"),
        ] {
            let output = solve_steps(&mut engine, equation, "x").unwrap();
            let event = output.steps.iter().find(|step| step.rule == rule).unwrap();
            assert_eq!(event.expr, transformed);
            assert!(output
                .steps
                .iter()
                .any(|step| step.rule == "equation-verify"));
        }

        let composite = solve_steps(&mut engine, "Exp(2*x)==2", "x").unwrap();
        assert!(!composite
            .steps
            .iter()
            .any(|step| step.rule.starts_with("equation-invert-")));
    }

    #[test]
    fn system_steps_cover_discovery_parameters_inconsistency_and_verification() {
        let mut engine = RustEngine::spawn().unwrap();
        let solved = solve_system_steps(&mut engine, &["x+y==3", "x-y==1"], &[]).unwrap();
        for rule in [
            "equation-system-variables",
            "equation-system-eliminate",
            "equation-system-verify",
            "equation-system-result",
        ] {
            assert!(solved.steps.iter().any(|step| step.rule == rule));
        }
        assert_eq!(solved.result.variable_source, VariableSource::Inferred);

        let parametric = solve_system_steps(&mut engine, &["x+y==3"], &["x", "y"]).unwrap();
        assert_eq!(
            parametric.result.completeness,
            SolveCompleteness::Parametric
        );
        assert!(parametric.steps.iter().any(|step| {
            step.rule == "equation-system-branch" && step.why.contains("自由参数")
        }));

        let inconsistent =
            solve_system_steps(&mut engine, &["x+y==3", "x+y==4"], &["x", "y"]).unwrap();
        assert_eq!(inconsistent.result.status, SolveStatus::NoSolution);
        assert!(inconsistent
            .steps
            .iter()
            .any(|step| step.rule == "equation-system-inconsistent"));
    }

    #[test]
    fn rational_and_radical_steps_expose_domain_and_final_verification() {
        let mut engine = RustEngine::spawn().unwrap();
        let rational = solve_steps(&mut engine, "1/(x-1)==2", "x").unwrap();
        assert!(rational
            .steps
            .iter()
            .any(|step| step.rule == "equation-domain-exclusion"));
        assert!(rational
            .steps
            .iter()
            .any(|step| { step.rule == "equation-verify" && step.why.contains("排除集") }));

        let radical = solve_steps(&mut engine, "Sqrt(x+1)==3", "x").unwrap();
        assert!(radical
            .steps
            .iter()
            .any(|step| step.rule == "equation-radical-square"));
        assert!(radical
            .steps
            .iter()
            .any(|step| step.rule == "equation-verify"));

        let composite = solve_steps(&mut engine, "Sin(2*x)==0", "x").unwrap();
        assert_eq!(
            composite.result.completeness,
            SolveCompleteness::Representative
        );
        assert!(composite.result.families.is_empty());
    }
}
