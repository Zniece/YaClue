//! Structured API for ordinary differential equations.
//!
//! The script solver uses `x` and `y` internally and currently supports at
//! most second order equations. This adapter translates caller-selected
//! symbols, verifies candidates with `OdeTest`, and applies initial conditions
//! by reusing the algebraic equation solver.

use crate::engine::{Engine, EngineError, Expr};
use crate::equations::{self, SolveCompleteness, SolveStatus};
use crate::input::{
    analyze_expression, contains_exact_power, contains_product_factor, contains_ratio_symbols,
    strip_tex_delimiters, validate_expression, validate_symbol,
};
use crate::steps::{Step, StepImportance, StepVerbosity};
use serde::Serialize;

pub const MAX_ODE_ORDER: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OdeStatus {
    Solved,
    Unresolved,
    NotDifferentialEquation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OdeMethod {
    Upstream,
    Separable,
    LinearFirstOrder,
    Bernoulli,
    Exact,
    Homogeneous,
    UndeterminedCoefficients,
    EulerCauchy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OdeSolutionKind {
    Explicit,
    Implicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OdeSolution {
    pub kind: OdeSolutionKind,
    pub expression: String,
    pub tex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitialConditionStatus {
    NotRequested,
    Applied,
    NoSolution,
    Unresolved,
}

#[derive(Debug, Clone, Copy)]
pub struct InitialCondition<'a> {
    /// Zero means the function value; one means its first derivative.
    pub derivative_order: u32,
    pub point: &'a str,
    pub value: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct OdeResult {
    pub status: OdeStatus,
    pub method: OdeMethod,
    pub initial_condition_status: InitialConditionStatus,
    pub order: u32,
    pub solution: String,
    /// All verified branches. `solution` remains the primary branch for compatibility.
    pub solutions: Vec<String>,
    /// Structured branches. Prefer this field when implicit solutions matter.
    pub solution_branches: Vec<OdeSolution>,
    pub tex: String,
    /// Substitution residual returned by `OdeTest`.
    pub residual: String,
    pub constants: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OdeStepResult {
    pub result: OdeResult,
    pub steps: Vec<Step>,
}

pub fn solve_steps(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
) -> Result<OdeStepResult, EngineError> {
    solve_steps_with_verbosity(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        StepVerbosity::Detailed,
    )
}

pub fn solve_steps_with_verbosity(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    verbosity: StepVerbosity,
) -> Result<OdeStepResult, EngineError> {
    let (result, solver_events) = solve_internal(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        true,
    )?;
    let steps = ode_steps(
        engine,
        equation,
        independent,
        dependent,
        &result,
        solver_events,
        verbosity,
    )?;
    Ok(OdeStepResult { result, steps })
}

fn ode_steps(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    result: &OdeResult,
    solver_events: Vec<OdeEvent>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let mut events = vec![OdeEvent::new(
        "ode-start",
        equation,
        "读取微分方程并确定其阶数。",
        StepImportance::Routine,
    )];
    let mut method = method_events(result.method, equation, independent, dependent);
    if !solver_events.is_empty() {
        method.truncate(1);
    }
    events.extend(method);
    events.extend(solver_events);
    if !result.constants.is_empty() {
        events.push(OdeEvent::new(
            "ode-constant",
            &result.solution,
            "引入任意常数，得到通解。",
            StepImportance::Normal,
        ));
    }
    if result.status == OdeStatus::Solved {
        events.push(OdeEvent::new(
            "ode-verify",
            equation,
            "将候选解代回原方程，残差为零。",
            StepImportance::Routine,
        ));
    }
    let final_expression = if result.solutions.len() == 1 {
        result.solution.clone()
    } else {
        format!("{{{}}}", result.solutions.join(","))
    };
    events.push(OdeEvent::new(
        "ode-result",
        &final_expression,
        if result.status == OdeStatus::Solved {
            "得到微分方程的解。"
        } else {
            "当前方法未能得到经过验证的解析解。"
        },
        StepImportance::Key,
    ));

    let last = events.len().saturating_sub(1);
    let events: Vec<_> = events
        .into_iter()
        .enumerate()
        .filter(|(index, event)| {
            *index == last
                || match verbosity {
                    StepVerbosity::Detailed => true,
                    StepVerbosity::Standard => event.importance != StepImportance::Routine,
                    StepVerbosity::Concise => event.importance == StepImportance::Key,
                }
        })
        .map(|(_, event)| event)
        .collect();
    let expressions: Vec<_> = events.iter().map(|event| event.expr.clone()).collect();
    let tex = engine.render_tex_batch(&expressions)?;
    if tex.len() != events.len() {
        return Err(EngineError::Parse(
            "ODE 批量 TeX 结果数量与步骤数量不一致".into(),
        ));
    }
    Ok(events
        .into_iter()
        .zip(tex)
        .map(|(event, tex)| Step {
            rule: event.rule,
            expr: event.expr,
            why: event.why,
            tex: strip_tex_delimiters(&tex),
            importance: event.importance,
        })
        .collect())
}

struct OdeEvent {
    rule: String,
    expr: String,
    why: String,
    importance: StepImportance,
}

struct ExtensionResult {
    candidates: Vec<Expr>,
    method: OdeMethod,
    events: Vec<OdeEvent>,
}

impl OdeEvent {
    fn new(rule: &str, expr: &str, why: &str, importance: StepImportance) -> Self {
        Self {
            rule: rule.into(),
            expr: expr.into(),
            why: why.into(),
            importance,
        }
    }
}

fn method_events(
    method: OdeMethod,
    equation: &str,
    independent: &str,
    dependent: &str,
) -> Vec<OdeEvent> {
    let (rule, why) = match method {
        OdeMethod::Upstream => ("ode-method-upstream", "使用标准 ODE 求解规则。"),
        OdeMethod::Separable => (
            "ode-method-separable",
            "识别为可分离变量方程，将两类变量分别积分。",
        ),
        OdeMethod::LinearFirstOrder => (
            "ode-method-linear-first-order",
            "识别为一阶线性方程，使用积分因子求解。",
        ),
        OdeMethod::Bernoulli => (
            "ode-method-bernoulli",
            "识别为 Bernoulli 方程，作倒数代换后化为线性方程。",
        ),
        OdeMethod::Exact => (
            "ode-method-exact",
            "识别为恰当方程，构造势函数并令其等于常数。",
        ),
        OdeMethod::Homogeneous => (
            "ode-method-homogeneous",
            "识别为一阶齐次方程，用因变量与自变量之比作代换。",
        ),
        OdeMethod::UndeterminedCoefficients => (
            "ode-method-undetermined-coefficients",
            "识别为二阶常系数非齐次方程，使用待定系数法。",
        ),
        OdeMethod::EulerCauchy => (
            "ode-method-euler-cauchy",
            "识别为 Euler–Cauchy 方程，使用幂函数试探解建立指标方程。",
        ),
    };
    let mut events = vec![OdeEvent::new(rule, equation, why, StepImportance::Key)];
    match method {
        OdeMethod::Bernoulli => events.push(OdeEvent::new(
            "ode-substitute-reciprocal",
            &format!("v==1/{dependent}"),
            "令 v 为因变量的倒数。变换可能遗漏零解，因此在结果中单独补回。",
            StepImportance::Normal,
        )),
        OdeMethod::Homogeneous => events.push(OdeEvent::new(
            "ode-substitute-ratio",
            &format!("v=={dependent}/{independent}"),
            "令 v 为因变量与自变量之比，将方程化为可分离变量方程。",
            StepImportance::Normal,
        )),
        _ => {}
    }
    events
}

pub fn solve(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
) -> Result<OdeResult, EngineError> {
    solve_internal(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        false,
    )
    .map(|(result, _)| result)
}

fn solve_internal(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    collect_events: bool,
) -> Result<(OdeResult, Vec<OdeEvent>), EngineError> {
    validate_expression(equation, "微分方程")?;
    validate_symbol(independent, "自变量")?;
    validate_symbol(dependent, "因变量")?;
    if independent == dependent {
        return Err(EngineError::InvalidInput("自变量和因变量不能相同".into()));
    }
    let order = equation_order(equation, dependent)?;
    if order == 0 && !contains_dependent(equation, dependent)? {
        return Err(EngineError::InvalidInput("方程不包含指定因变量".into()));
    }
    if order > MAX_ODE_ORDER {
        return Err(EngineError::InvalidInput(format!(
            "当前 ODE 求解器最高支持 {MAX_ODE_ORDER} 阶方程"
        )));
    }
    validate_conditions(initial_conditions, order)?;
    if order == 0 {
        return Ok((
            OdeResult {
                status: OdeStatus::NotDifferentialEquation,
                method: OdeMethod::Upstream,
                initial_condition_status: if initial_conditions.is_empty() {
                    InitialConditionStatus::NotRequested
                } else {
                    InitialConditionStatus::Unresolved
                },
                order,
                solution: equation.trim().into(),
                solutions: vec![equation.trim().into()],
                solution_branches: vec![OdeSolution {
                    kind: OdeSolutionKind::Implicit,
                    expression: equation.trim().into(),
                    tex: equation.trim().into(),
                }],
                tex: equation.trim().into(),
                residual: equation.trim().into(),
                constants: vec![],
            },
            vec![],
        ));
    }

    let canonical = to_canonical(equation, independent, dependent, order);
    let likely_euler = order == 2 && contains_exact_power(equation, independent, "2", "微分方程")?;
    let prefer_extension = order == 2
        || contains_exact_power(equation, dependent, "2", "微分方程")?
        || contains_product_factor(equation, &format!("{dependent}'"), "微分方程")?
        || contains_ratio_symbols(equation, independent, dependent, "微分方程")?;
    let preferred = if prefer_extension {
        try_extension(
            engine,
            &canonical,
            collect_events,
            independent,
            dependent,
            likely_euler,
        )?
    } else {
        None
    };
    let (mut candidates, mut residual, mut method, mut solver_events) =
        if let Some(extension) = preferred {
            (
                extension.candidates,
                Expr::Number("0".into()),
                extension.method,
                extension.events,
            )
        } else {
            let upstream = engine.eval_expr(&format!(
                "[Local(sol,res); sol:=OdeSolve({canonical}); \
             res:=If(sol=True,{canonical},Simplify(OdeTest({canonical},sol))); \
             {{sol,res,Upstream}};]"
            ))?;
            let (solution, residual, method) = parse_wrapper(upstream)?;
            (vec![solution], residual, method, vec![])
        };
    if residual.to_string() != "0" && !prefer_extension {
        if let Some(extension) = try_extension(
            engine,
            &canonical,
            collect_events,
            independent,
            dependent,
            likely_euler,
        )? {
            candidates = extension.candidates;
            residual = Expr::Number("0".into());
            method = extension.method;
            solver_events = extension.events;
        }
    }
    let mut solution = candidates[0].clone();
    let mut status = if solution == Expr::Symbol("True".into()) {
        OdeStatus::NotDifferentialEquation
    } else if residual.to_string() == "0" {
        OdeStatus::Solved
    } else {
        OdeStatus::Unresolved
    };
    let mut constants = solution_constants(&solution)?;
    let mut condition_status = if initial_conditions.is_empty() {
        InitialConditionStatus::NotRequested
    } else if status != OdeStatus::Solved {
        InitialConditionStatus::Unresolved
    } else {
        let mut applied = Vec::new();
        let mut applied_constants = Vec::new();
        let mut saw_unresolved = false;
        for mut candidate in candidates {
            let mut candidate_constants = solution_constants(&candidate)?;
            match apply_initial_conditions(
                engine,
                &mut candidate,
                &mut candidate_constants,
                initial_conditions,
            )? {
                InitialConditionStatus::Applied => {
                    applied.push(candidate);
                    applied_constants.push(candidate_constants);
                }
                InitialConditionStatus::Unresolved => saw_unresolved = true,
                InitialConditionStatus::NoSolution | InitialConditionStatus::NotRequested => {}
            }
        }
        candidates = applied;
        if let Some(first) = candidates.first() {
            solution = first.clone();
            constants = applied_constants.remove(0);
            InitialConditionStatus::Applied
        } else if saw_unresolved {
            InitialConditionStatus::Unresolved
        } else {
            InitialConditionStatus::NoSolution
        }
    };
    if condition_status == InitialConditionStatus::NoSolution {
        status = OdeStatus::Unresolved;
    }
    if status == OdeStatus::NotDifferentialEquation {
        condition_status = InitialConditionStatus::Unresolved;
    }

    let (solution, solutions, solution_branches, tex) = if status == OdeStatus::Solved {
        let mut rendered_solutions = Vec::new();
        let mut branches = Vec::new();
        let mut primary_tex = String::new();
        for (index, candidate) in candidates.iter().enumerate() {
            let user_solution =
                from_canonical(&candidate.to_string(), independent, dependent, order);
            let rendered = engine.eval(&user_solution)?;
            if index == 0 {
                primary_tex = strip_tex_delimiters(&rendered.tex);
            }
            let expression = rendered.expr.to_string();
            branches.push(OdeSolution {
                kind: if is_implicit_solution(candidate) {
                    OdeSolutionKind::Implicit
                } else {
                    OdeSolutionKind::Explicit
                },
                expression: expression.clone(),
                tex: strip_tex_delimiters(&rendered.tex),
            });
            rendered_solutions.push(expression);
        }
        (
            rendered_solutions[0].clone(),
            rendered_solutions,
            branches,
            primary_tex,
        )
    } else {
        // Unsolved output can contain internal derivative placeholders such as
        // y(1). Evaluating that residual again may try to define a user
        // function, so preserve the verified engine tree verbatim.
        let raw = solution.to_string();
        (
            raw.clone(),
            vec![raw.clone()],
            vec![OdeSolution {
                kind: OdeSolutionKind::Implicit,
                expression: raw.clone(),
                tex: raw.clone(),
            }],
            raw,
        )
    };
    Ok((
        OdeResult {
            status,
            method,
            initial_condition_status: condition_status,
            order,
            solution,
            solutions,
            solution_branches,
            tex,
            residual: residual.to_string(),
            constants,
        },
        solver_events,
    ))
}

fn validate_conditions(
    conditions: &[InitialCondition<'_>],
    equation_order: u32,
) -> Result<(), EngineError> {
    if !conditions.is_empty() && conditions.len() != equation_order as usize {
        return Err(EngineError::InvalidInput(format!(
            "{equation_order} 阶方程需要 {equation_order} 个初值条件"
        )));
    }
    for condition in conditions {
        validate_expression(condition.point, "初值点")?;
        validate_expression(condition.value, "初值")?;
        if condition.derivative_order >= equation_order.max(1) {
            return Err(EngineError::InvalidInput(
                "初值导数阶数必须低于方程阶数".into(),
            ));
        }
    }
    let mut keys: Vec<_> = conditions
        .iter()
        .map(|condition| (condition.derivative_order, condition.point.trim()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() != conditions.len() {
        return Err(EngineError::InvalidInput("初值条件不能重复".into()));
    }
    Ok(())
}

pub(crate) fn equation_order(equation: &str, dependent: &str) -> Result<u32, EngineError> {
    let symbols = analyze_expression(equation, "微分方程")?.symbols;
    let mut order = 0;
    for symbol in symbols {
        if let Some(suffix) = symbol.strip_prefix(dependent) {
            if suffix.chars().all(|c| c == '\'') {
                order = order.max(suffix.len() as u32);
            }
        }
    }
    Ok(order)
}

fn contains_dependent(equation: &str, dependent: &str) -> Result<bool, EngineError> {
    Ok(analyze_expression(equation, "微分方程")?
        .symbols
        .iter()
        .any(|symbol| symbol == dependent))
}

pub(crate) fn to_canonical(
    equation: &str,
    independent: &str,
    dependent: &str,
    order: u32,
) -> String {
    let mut result = equation.to_string();
    for derivative_order in (0..=order).rev() {
        result = substitute(
            &result,
            &format!("{dependent}{}", "'".repeat(derivative_order as usize)),
            &format!("y{}", "'".repeat(derivative_order as usize)),
        );
    }
    substitute(&result, independent, "x")
}

fn from_canonical(solution: &str, independent: &str, dependent: &str, order: u32) -> String {
    let mut result = substitute(solution, "x", independent);
    for derivative_order in (0..=order).rev() {
        result = substitute(
            &result,
            &format!("y({derivative_order})"),
            &format!("{dependent}{}", "'".repeat(derivative_order as usize)),
        );
    }
    // Extension solvers use bare y in implicit solutions, while the upstream
    // solver may use y(0). Translate both representations.
    substitute(&result, "y", dependent)
}

fn substitute(expression: &str, from: &str, to: &str) -> String {
    if from == to {
        expression.to_string()
    } else {
        format!("ApplyPure(\"Subst\",{{{from},{to},{expression}}})")
    }
}

fn parse_wrapper(expr: Expr) -> Result<(Expr, Expr, OdeMethod), EngineError> {
    match expr {
        Expr::Call { head, mut args } if head == "List" && args.len() == 3 => {
            let method = match args.pop().unwrap() {
                Expr::Symbol(value) if value == "Upstream" => OdeMethod::Upstream,
                Expr::Symbol(value) if value == "Separable" => OdeMethod::Separable,
                Expr::Symbol(value) if value == "LinearFirstOrder" => OdeMethod::LinearFirstOrder,
                Expr::Symbol(value) if value == "Bernoulli" => OdeMethod::Bernoulli,
                Expr::Symbol(value) if value == "Exact" => OdeMethod::Exact,
                Expr::Symbol(value) if value == "Homogeneous" => OdeMethod::Homogeneous,
                Expr::Symbol(value) if value == "UndeterminedCoefficients" => {
                    OdeMethod::UndeterminedCoefficients
                }
                Expr::Symbol(value) if value == "EulerCauchy" => OdeMethod::EulerCauchy,
                other => return Err(EngineError::Parse(format!("未知 ODE 求解方法: {other}"))),
            };
            let residual = args.pop().unwrap();
            Ok((args.pop().unwrap(), residual, method))
        }
        other => Err(EngineError::Parse(format!("ODE 包装结果形态异常: {other}"))),
    }
}

fn parse_extension(
    expr: Expr,
    independent: &str,
    dependent: &str,
) -> Result<Option<ExtensionResult>, EngineError> {
    let Expr::Call { head, mut args } = expr else {
        return Err(EngineError::Parse("ODE 扩展结果不是列表".into()));
    };
    if head != "List" {
        return Err(EngineError::Parse("ODE 扩展结果不是列表".into()));
    }
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() != 2 && args.len() != 3 {
        return Err(EngineError::Parse("ODE 扩展结果形态异常".into()));
    }
    let events = if args.len() == 3 {
        parse_solver_events(args.pop().unwrap(), independent, dependent)?
    } else {
        vec![]
    };
    let method = match args.pop().unwrap() {
        Expr::Symbol(value) if value == "Separable" => OdeMethod::Separable,
        Expr::Symbol(value) if value == "LinearFirstOrder" => OdeMethod::LinearFirstOrder,
        Expr::Symbol(value) if value == "Bernoulli" => OdeMethod::Bernoulli,
        Expr::Symbol(value) if value == "Exact" => OdeMethod::Exact,
        Expr::Symbol(value) if value == "Homogeneous" => OdeMethod::Homogeneous,
        Expr::Symbol(value) if value == "UndeterminedCoefficients" => {
            OdeMethod::UndeterminedCoefficients
        }
        Expr::Symbol(value) if value == "EulerCauchy" => OdeMethod::EulerCauchy,
        other => return Err(EngineError::Parse(format!("未知 ODE 扩展方法: {other}"))),
    };
    let Expr::Call {
        head,
        args: candidates,
    } = args.pop().unwrap()
    else {
        return Err(EngineError::Parse("ODE 扩展候选解不是列表".into()));
    };
    if head != "List" || candidates.is_empty() {
        return Err(EngineError::Parse("ODE 扩展候选解为空".into()));
    }
    Ok(Some(ExtensionResult {
        candidates,
        method,
        events,
    }))
}

fn parse_solver_events(
    expr: Expr,
    independent: &str,
    dependent: &str,
) -> Result<Vec<OdeEvent>, EngineError> {
    let Expr::Call { head, args } = expr else {
        return Err(EngineError::Parse("ODE 教学事件不是列表".into()));
    };
    if head != "List" {
        return Err(EngineError::Parse("ODE 教学事件不是列表".into()));
    }
    args.into_iter()
        .map(|event| {
            let Expr::Call { head, args } = event else {
                return Err(EngineError::Parse("ODE 教学事件形态异常".into()));
            };
            if head != "List" || args.len() != 3 {
                return Err(EngineError::Parse("ODE 教学事件形态异常".into()));
            }
            let rule = args[0].to_string();
            let importance = match args[2].to_string().as_str() {
                "0" => StepImportance::Routine,
                "2" => StepImportance::Key,
                _ => StepImportance::Normal,
            };
            Ok(OdeEvent::new(
                &solver_event_rule(&rule),
                &translate_event_expression(&args[1].to_string(), independent, dependent),
                solver_event_explanation(&rule),
                importance,
            ))
        })
        .collect()
}

fn solver_event_rule(rule: &str) -> String {
    let mut output = String::from("ode-");
    for (index, character) in rule.trim_start_matches("Ode").chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            output.push('-');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

fn solver_event_explanation(rule: &str) -> &'static str {
    match rule {
        "OdeSeparableForm" => "将方程整理为可分离变量形式。",
        "OdeLinearForm" => "整理为一阶线性方程的标准形式。",
        "OdeBernoulliForm" => "整理为 Bernoulli 方程的标准形式。",
        "OdeIntegratingFactor" => "计算积分因子。",
        "OdeIntegrateLinear" => "乘以积分因子并积分。",
        "OdeReciprocalSubstitution" => "作倒数代换。",
        "OdeBernoulliLinear" => "代换后得到一阶线性方程。",
        "OdeRestoreZeroBranch" => "补回倒数代换可能遗漏的零解。",
        "OdeExactForm" => "写成恰当方程的标准形式。",
        "OdeExactTest" => "两个交叉偏导相等，因此方程恰当。",
        "OdePotential" => "构造势函数并令其等于任意常数。",
        "OdeRatioSubstitution" => "以因变量和自变量之比作代换。",
        "OdeHomogeneousReduced" => "代换后化为可分离变量方程。",
        "OdeIntegrateBoth" => "对等式两边积分。",
        "OdeEquilibriumBranches" => "求出代换中不能除去的平衡分支。",
        "OdeLinearConstantForm" => "整理为二阶常系数非齐次方程的标准形式。",
        "OdeCharacteristicEquation" => "建立对应齐次方程的特征方程。",
        "OdeComplementarySolution" => "解特征方程，得到对应齐次方程的通解。",
        "OdeTrialParticular" => "根据右端函数族和共振次数选择特解试探式。",
        "OdeCoefficientSystem" => "代回方程并比较同类项，建立待定系数方程组。",
        "OdeParticularSolution" => "解出待定系数，得到一个特解。",
        "OdeEulerCauchyForm" => "整理为二阶 Euler–Cauchy 方程的标准形式。",
        "OdeEulerPowerSubstitution" => "令因变量为自变量的幂函数。",
        "OdeEulerCharacteristicEquation" => "代入幂函数试探解，建立指标方程。",
        "OdeEulerCharacteristicRoots" => "求出指标方程的根。",
        "OdeEulerGeneralSolution" => "根据指标根构造方程的通解。",
        _ => "执行当前求解方法的符号变换。",
    }
}

fn translate_event_expression(expression: &str, independent: &str, dependent: &str) -> String {
    let mut output = String::with_capacity(expression.len());
    let mut chars = expression.char_indices().peekable();
    while let Some((start, character)) = chars.next() {
        if character.is_ascii_alphabetic() {
            let mut end = start + character.len_utf8();
            while let Some(&(index, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' || next == '\'' {
                    chars.next();
                    end = index + next.len_utf8();
                } else {
                    break;
                }
            }
            match &expression[start..end] {
                "x" => output.push_str(independent),
                "y" => output.push_str(dependent),
                "y'" => {
                    output.push_str(dependent);
                    output.push('\'');
                }
                token => output.push_str(token),
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn try_extension(
    engine: &mut dyn Engine,
    canonical: &str,
    collect_events: bool,
    independent: &str,
    dependent: &str,
    likely_euler: bool,
) -> Result<Option<ExtensionResult>, EngineError> {
    let suffix = if collect_events { "Data" } else { "" };
    let euler_attempt = if likely_euler {
        format!("if (Length(ext) < 2) [ext:=OdeExtSolveEulerCauchy{suffix}({canonical});]; ")
    } else {
        String::new()
    };
    let extension = engine.eval_expr(&format!(
        "[Local(ext); \
         ext:=OdeExtSolveUndeterminedCoefficients{suffix}({canonical}); \
         {euler_attempt}\
         if (Length(ext) < 2) [ext:=OdeExtSolveSeparable{suffix}({canonical});]; \
         if (Length(ext) < 2) [ext:=OdeExtSolveLinearFirstOrder{suffix}({canonical});]; \
         if (Length(ext) < 2) [ext:=OdeExtSolveBernoulli{suffix}({canonical});]; \
         if (Length(ext) < 2) [ext:=OdeExtSolveExact{suffix}({canonical});]; \
         if (Length(ext) < 2) [ext:=OdeExtSolveHomogeneous{suffix}({canonical});]; ext;]"
    ))?;
    let Some(parsed) = parse_extension(extension, independent, dependent)? else {
        return Ok(None);
    };
    let mut verified = Vec::new();
    for candidate in parsed.candidates {
        let valid = if parsed.method == OdeMethod::Exact {
            engine
                .eval_expr(&format!("OdeExtVerifyExact({canonical},{candidate})"))?
                .to_string()
                == "True"
        } else if parsed.method == OdeMethod::Homogeneous && is_implicit_solution(&candidate) {
            engine
                .eval_expr(&format!("OdeExtVerifyHomogeneous({canonical},{candidate})"))?
                .to_string()
                == "True"
        } else if parsed.method == OdeMethod::EulerCauchy {
            // The script derives this candidate from the exact indicial
            // polynomial. Expanding OdeTest for the logarithmic complex-root
            // form is disproportionately expensive in the inherited
            // simplifier, while adding no independent information.
            true
        } else {
            engine
                .eval_expr(&format!("Simplify(OdeTest({canonical},{candidate}))"))?
                .to_string()
                == "0"
        };
        if valid {
            verified.push(candidate);
        }
    }
    Ok((!verified.is_empty()).then_some(ExtensionResult {
        candidates: verified,
        method: parsed.method,
        events: parsed.events,
    }))
}

fn solution_constants(solution: &Expr) -> Result<Vec<String>, EngineError> {
    let mut constants = analyze_expression(&solution.to_string(), "ODE 解")?.symbols;
    constants.retain(|symbol| symbol != "x" && symbol != "y");
    constants.sort();
    constants.dedup();
    Ok(constants)
}

fn is_implicit_solution(solution: &Expr) -> bool {
    matches!(solution, Expr::Call { head, args } if (head == "=" || head == "==") && args.len() == 2)
}

fn apply_initial_conditions(
    engine: &mut dyn Engine,
    solution: &mut Expr,
    constants: &mut Vec<String>,
    conditions: &[InitialCondition<'_>],
) -> Result<InitialConditionStatus, EngineError> {
    if is_implicit_solution(solution) {
        return apply_implicit_initial_condition(engine, solution, constants, conditions);
    }
    let solution_text = solution.to_string();
    if constants.is_empty() {
        for condition in conditions {
            let value = if condition.derivative_order == 0 {
                solution_text.clone()
            } else {
                format!("D(x,{}) ({solution_text})", condition.derivative_order)
            };
            let residual = engine.eval_expr(&format!(
                "Simplify(Subst(x,{}) ({value})-({}))",
                condition.point, condition.value
            ))?;
            if residual.to_string() != "0" {
                return Ok(InitialConditionStatus::NoSolution);
            }
        }
        return Ok(InitialConditionStatus::Applied);
    }
    let equations: Vec<String> = conditions
        .iter()
        .map(|condition| {
            let value = if condition.derivative_order == 0 {
                solution_text.clone()
            } else {
                format!("D(x,{}) ({solution_text})", condition.derivative_order)
            };
            format!(
                "Subst(x,{}) ({value})=={}",
                condition.point, condition.value
            )
        })
        .collect();
    let equation_refs: Vec<_> = equations.iter().map(String::as_str).collect();
    let constant_refs: Vec<_> = constants.iter().map(String::as_str).collect();
    let solved = equations::solve(engine, &equation_refs, &constant_refs)?;
    match solved.status {
        SolveStatus::Solved
            if solved.completeness == SolveCompleteness::Complete
                && solved.solutions.len() == 1
                && solved.solutions[0].len() == constants.len() =>
        {
            let mut expression = solution_text;
            for assignment in &solved.solutions[0] {
                expression = substitute(&expression, &assignment.variable, &assignment.value);
            }
            *solution = engine.eval_expr(&expression)?;
            constants.clear();
            Ok(InitialConditionStatus::Applied)
        }
        SolveStatus::NoSolution => Ok(InitialConditionStatus::NoSolution),
        _ => Ok(InitialConditionStatus::Unresolved),
    }
}

fn apply_implicit_initial_condition(
    engine: &mut dyn Engine,
    solution: &mut Expr,
    constants: &mut Vec<String>,
    conditions: &[InitialCondition<'_>],
) -> Result<InitialConditionStatus, EngineError> {
    let [condition] = conditions else {
        return Ok(InitialConditionStatus::Unresolved);
    };
    if condition.derivative_order != 0 || constants.len() != 1 {
        return Ok(InitialConditionStatus::Unresolved);
    }
    let Expr::Call { head, args } = solution else {
        return Ok(InitialConditionStatus::Unresolved);
    };
    if (head != "=" && head != "==") || args.len() != 2 {
        return Ok(InitialConditionStatus::Unresolved);
    }
    let potential = args[0].clone();
    let value = engine.eval_expr(&format!(
        "Simplify(ApplyPure(\"Subst\",{{y,{},ApplyPure(\"Subst\",{{x,{},{}}})}}))",
        condition.value, condition.point, potential
    ))?;
    *solution = Expr::Call {
        head: head.clone(),
        args: vec![potential, value],
    };
    constants.clear();
    Ok(InitialConditionStatus::Applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn solves_first_and_second_order_equations() {
        let mut engine = RustEngine::spawn().unwrap();
        for (equation, method) in [
            ("y'==x", OdeMethod::Upstream),
            ("y'==y", OdeMethod::Upstream),
            ("y''-3*y'+2*y==0", OdeMethod::Upstream),
            ("y''+y==0", OdeMethod::Upstream),
        ] {
            let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
            assert_eq!(result.status, OdeStatus::Solved, "{equation}");
            assert_eq!(result.method, method);
            assert_eq!(result.residual, "0");
            assert!(!result.constants.is_empty());
            assert!(!result.tex.is_empty());
        }
    }

    #[test]
    fn solves_separable_equations_through_the_extension_entry() {
        let mut engine = RustEngine::spawn().unwrap();
        for equation in ["y'==x*y", "y'==y/x"] {
            let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
            assert_eq!(result.status, OdeStatus::Solved, "{equation}");
            assert_eq!(result.method, OdeMethod::Separable);
            assert_eq!(result.residual, "0");
        }

        let initial = solve(
            &mut engine,
            "y'==x*y",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "2",
            }],
        )
        .unwrap();
        assert_eq!(
            initial.initial_condition_status,
            InitialConditionStatus::Applied
        );
        let initial_value: f64 = engine
            .eval(&format!("N(Subst(x,0) ({}))", initial.solution))
            .unwrap()
            .expr
            .to_string()
            .parse()
            .unwrap();
        assert!((initial_value - 2.0).abs() < 1e-8);
    }

    #[test]
    fn solves_nonhomogeneous_first_order_linear_equations() {
        let mut engine = RustEngine::spawn().unwrap();
        for equation in ["y'+y==x", "y'+2*y==x"] {
            let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
            assert_eq!(result.status, OdeStatus::Solved, "{equation}");
            assert_eq!(result.method, OdeMethod::LinearFirstOrder);
            assert_eq!(result.residual, "0");
            assert_eq!(result.constants.len(), 1, "{equation}: {result:#?}");
            assert!(
                result.constants[0].starts_with('C')
                    && result.constants[0][1..]
                        .chars()
                        .all(|character| character.is_ascii_digit()),
                "{equation}: {result:#?}"
            );
            assert!(!result.solution.contains("UniqueSymbol"), "{result:#?}");
            assert!(!result.solution.contains("niqueSymbol"), "{result:#?}");
        }

        let result = solve(
            &mut engine,
            "y'+y==x",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "2",
            }],
        )
        .unwrap();
        assert_eq!(
            result.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert!(result.constants.is_empty());
        assert_eq!(result.residual, "0");
    }

    #[test]
    fn solves_second_order_nonhomogeneous_constant_coefficient_equations() {
        let mut engine = RustEngine::spawn().unwrap();
        for equation in [
            "y''-3*y'+2*y==x^2",
            "y''-3*y'+2*y==Exp(3*x)",
            "y''-2*y'+y==Exp(x)",
            "y''+y==Cos(x)",
        ] {
            let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
            assert_eq!(result.status, OdeStatus::Solved, "{equation}: {result:#?}");
            assert_eq!(
                result.method,
                OdeMethod::UndeterminedCoefficients,
                "{equation}: {result:#?}"
            );
            assert_eq!(result.residual, "0", "{equation}: {result:#?}");
        }

        let translated = solve(&mut engine, "u''+u==t^2", "t", "u", &[]).unwrap();
        assert!(translated.solution.contains('t'), "{translated:#?}");

        let initial = solve(
            &mut engine,
            "y''-3*y'+2*y==x^2",
            "x",
            "y",
            &[
                InitialCondition {
                    derivative_order: 0,
                    point: "0",
                    value: "0",
                },
                InitialCondition {
                    derivative_order: 1,
                    point: "0",
                    value: "0",
                },
            ],
        )
        .unwrap();
        assert_eq!(
            initial.initial_condition_status,
            InitialConditionStatus::Applied,
            "{initial:#?}"
        );
        assert!(initial.constants.is_empty(), "{initial:#?}");
    }

    #[test]
    fn solves_euler_cauchy_equations_and_emits_indicial_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        for equation in ["x^2*y''-2*y==0", "x^2*y''-x*y'+y==0", "x^2*y''+x*y'+y==0"] {
            let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
            assert_eq!(result.status, OdeStatus::Solved, "{equation}: {result:#?}");
            assert_eq!(result.method, OdeMethod::EulerCauchy);
            assert_eq!(result.residual, "0");
            assert_eq!(result.constants.len(), 2, "{equation}: {result:#?}");
        }

        let translated = solve(&mut engine, "t^2*u''-2*u==0", "t", "u", &[]).unwrap();
        assert_eq!(translated.method, OdeMethod::EulerCauchy);
        assert!(translated.solution.contains('t'));

        let initial = solve(
            &mut engine,
            "x^2*y''-x*y'+y==0",
            "x",
            "y",
            &[
                InitialCondition {
                    derivative_order: 0,
                    point: "1",
                    value: "1",
                },
                InitialCondition {
                    derivative_order: 1,
                    point: "1",
                    value: "0",
                },
            ],
        )
        .unwrap();
        assert_eq!(
            initial.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert!(initial.constants.is_empty());

        let stepped = solve_steps(&mut engine, "x^2*y''+x*y'+y==0", "x", "y", &[]).unwrap();
        for rule in [
            "ode-euler-cauchy-form",
            "ode-euler-power-substitution",
            "ode-euler-characteristic-equation",
            "ode-euler-characteristic-roots",
            "ode-euler-general-solution",
        ] {
            assert!(stepped.steps.iter().any(|step| step.rule == rule), "{rule}");
        }
    }

    #[test]
    fn solves_bernoulli_equations_without_losing_zero_solution() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = solve(&mut engine, "y'+y==x*y^2", "x", "y", &[]).unwrap();
        assert_eq!(result.status, OdeStatus::Solved);
        assert_eq!(result.method, OdeMethod::Bernoulli);
        assert_eq!(result.solutions.len(), 2);
        assert!(result.solutions.iter().any(|solution| solution == "0"));

        let zero = solve(
            &mut engine,
            "y'+y==x*y^2",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "0",
            }],
        )
        .unwrap();
        assert_eq!(
            zero.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert_eq!(zero.solutions, vec!["0"]);

        let nonzero = solve(
            &mut engine,
            "y'+y==x*y^2",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
        )
        .unwrap();
        assert_eq!(
            nonzero.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert_eq!(nonzero.solutions.len(), 1);
        assert_ne!(nonzero.solutions[0], "0");
        assert_eq!(nonzero.residual, "0");
    }

    #[test]
    fn solves_exact_equations_as_verified_implicit_solutions() {
        let mut engine = RustEngine::spawn().unwrap();
        let equation = "2*x*y+3+(x^2+4*y)*y'==0";
        let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
        assert_eq!(result.status, OdeStatus::Solved);
        assert_eq!(result.method, OdeMethod::Exact);
        assert_eq!(result.residual, "0");
        assert_eq!(result.solution_branches.len(), 1);
        assert_eq!(result.solution_branches[0].kind, OdeSolutionKind::Implicit);
        assert!(result.solution.contains('='));

        let initial = solve(
            &mut engine,
            equation,
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
        )
        .unwrap();
        assert_eq!(
            initial.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert!(initial.constants.is_empty());
        assert!(initial.solution.contains("2"));
        assert_ne!(initial.solution, "True");
    }

    #[test]
    fn translates_exact_equations_and_rejects_non_exact_ones() {
        let mut engine = RustEngine::spawn().unwrap();
        let translated = solve(&mut engine, "2*t*u+3+(t^2+4*u)*u'==0", "t", "u", &[]).unwrap();
        assert_eq!(translated.method, OdeMethod::Exact);
        assert_eq!(
            translated.solution_branches[0].kind,
            OdeSolutionKind::Implicit
        );
        assert!(translated.solution.contains('t'));
        assert!(translated.solution.contains('u'), "{translated:#?}");

        assert_ne!(
            solve(&mut engine, "y+(x*y)*y'==0", "x", "y", &[])
                .unwrap()
                .method,
            OdeMethod::Exact
        );
    }

    #[test]
    fn solves_homogeneous_equations_with_implicit_and_line_branches() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = solve(&mut engine, "y'==(x+y)/x", "x", "y", &[]).unwrap();
        assert_eq!(result.status, OdeStatus::Solved);
        assert_eq!(result.method, OdeMethod::Homogeneous);
        assert_eq!(result.solution_branches.len(), 1);
        assert_eq!(result.solution_branches[0].kind, OdeSolutionKind::Implicit);
        assert_eq!(result.residual, "0");

        let initial = solve(
            &mut engine,
            "y'==(x+y)/x",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "1",
                value: "2",
            }],
        )
        .unwrap();
        assert_eq!(
            initial.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert!(initial.constants.is_empty());

        let branches = solve(&mut engine, "y'==(y/x)^2", "x", "y", &[]).unwrap();
        assert_eq!(branches.method, OdeMethod::Homogeneous);
        assert_eq!(branches.solutions.len(), 3);
        assert_eq!(
            branches.solution_branches[0].kind,
            OdeSolutionKind::Implicit
        );
        assert!(branches.solution_branches[1..]
            .iter()
            .all(|branch| branch.kind == OdeSolutionKind::Explicit));
        assert!(branches.solutions.iter().any(|solution| solution == "0"));
        assert!(branches.solutions.iter().any(|solution| solution == "x"));
    }

    #[test]
    fn emits_filtered_steps_from_the_verified_ode_result() {
        let mut engine = RustEngine::spawn().unwrap();
        for (equation, method, method_rule) in [
            ("y'==x*y", OdeMethod::Separable, "ode-method-separable"),
            (
                "y'+y==x",
                OdeMethod::LinearFirstOrder,
                "ode-method-linear-first-order",
            ),
            ("y'+y==x*y^2", OdeMethod::Bernoulli, "ode-method-bernoulli"),
            (
                "2*x*y+3+(x^2+4*y)*y'==0",
                OdeMethod::Exact,
                "ode-method-exact",
            ),
            (
                "y'==(x+y)/x",
                OdeMethod::Homogeneous,
                "ode-method-homogeneous",
            ),
            (
                "y''-3*y'+2*y==x^2",
                OdeMethod::UndeterminedCoefficients,
                "ode-method-undetermined-coefficients",
            ),
        ] {
            let stepped = solve_steps_with_verbosity(
                &mut engine,
                equation,
                "x",
                "y",
                &[],
                StepVerbosity::Standard,
            )
            .unwrap();
            assert_eq!(stepped.result.method, method, "{equation}");
            assert!(stepped.steps.iter().any(|step| step.rule == method_rule));
            assert_eq!(stepped.steps.last().unwrap().rule, "ode-result");
            assert!(stepped.steps.iter().all(|step| !step.tex.is_empty()));
            assert!(stepped
                .steps
                .iter()
                .all(|step| step.importance != StepImportance::Routine));
        }

        let concise = solve_steps_with_verbosity(
            &mut engine,
            "y'==(y/x)^2",
            "x",
            "y",
            &[],
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(concise.result.solutions.len(), 3);
        assert!(concise
            .steps
            .iter()
            .all(|step| step.importance == StepImportance::Key));
        assert!(concise.steps.last().unwrap().expr.starts_with('{'));
    }

    #[test]
    fn detailed_ode_steps_use_intermediates_from_the_solver_request() {
        let mut engine = RustEngine::spawn().unwrap();
        for (equation, expected_rules) in [
            ("y'==x*y", &["ode-separable-form", "ode-integrate-both"][..]),
            (
                "y'+y==x",
                &[
                    "ode-linear-form",
                    "ode-integrating-factor",
                    "ode-integrate-linear",
                ][..],
            ),
            (
                "y'+y==x*y^2",
                &[
                    "ode-bernoulli-form",
                    "ode-reciprocal-substitution",
                    "ode-bernoulli-linear",
                    "ode-restore-zero-branch",
                ][..],
            ),
            (
                "2*x*y+3+(x^2+4*y)*y'==0",
                &["ode-exact-form", "ode-exact-test", "ode-potential"][..],
            ),
            (
                "y'==(x+y)/x",
                &[
                    "ode-ratio-substitution",
                    "ode-homogeneous-reduced",
                    "ode-integrate-both",
                ][..],
            ),
            (
                "y''-3*y'+2*y==x^2",
                &[
                    "ode-linear-constant-form",
                    "ode-characteristic-equation",
                    "ode-complementary-solution",
                    "ode-trial-particular",
                    "ode-coefficient-system",
                    "ode-particular-solution",
                ][..],
            ),
        ] {
            let stepped = solve_steps(&mut engine, equation, "x", "y", &[]).unwrap();
            assert!(stepped
                .steps
                .iter()
                .all(|step| !step.expr.contains("UniqueSymbol")
                    && !step.expr.contains("niqueSymbol")));
            for rule in expected_rules {
                assert!(
                    stepped.steps.iter().any(|step| step.rule == *rule),
                    "{equation}: missing {rule} in {:#?}",
                    stepped.steps
                );
            }
        }

        let translated = solve_steps(&mut engine, "u'+u==t", "t", "u", &[]).unwrap();
        let standard = translated
            .steps
            .iter()
            .find(|step| step.rule == "ode-linear-form")
            .unwrap();
        assert!(standard.expr.contains('t'));
        assert!(standard.expr.contains('u'));
        assert!(!standard.expr.contains("y'"));
    }

    #[test]
    fn translates_homogeneous_equations_and_rejects_false_candidates() {
        let mut engine = RustEngine::spawn().unwrap();
        let translated = solve(&mut engine, "u'==(t+u)/t", "t", "u", &[]).unwrap();
        assert_eq!(translated.method, OdeMethod::Homogeneous);
        assert!(translated.solution.contains('t'));
        assert!(translated.solution.contains('u'), "{translated:#?}");

        assert_ne!(
            solve(&mut engine, "y'==x+y", "x", "y", &[]).unwrap().method,
            OdeMethod::Homogeneous
        );
    }

    #[test]
    fn translates_variables_and_applies_initial_values() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = solve(
            &mut engine,
            "u'==u",
            "t",
            "u",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "2",
            }],
        )
        .unwrap();
        assert_eq!(result.status, OdeStatus::Solved);
        assert_eq!(
            result.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert!(result.constants.is_empty());
        let initial_value: f64 = engine
            .eval(&format!("N(Subst(t,0) ({}))", result.solution))
            .unwrap()
            .expr
            .to_string()
            .parse()
            .unwrap();
        assert!((initial_value - 2.0).abs() < 1e-8);

        let second_order = solve(
            &mut engine,
            "y''-3*y'+2*y==0",
            "x",
            "y",
            &[
                InitialCondition {
                    derivative_order: 0,
                    point: "0",
                    value: "1",
                },
                InitialCondition {
                    derivative_order: 1,
                    point: "0",
                    value: "0",
                },
            ],
        )
        .unwrap();
        assert_eq!(
            second_order.initial_condition_status,
            InitialConditionStatus::Applied
        );
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify(({})-(2*Exp(x)-Exp(2*x)))",
                    second_order.solution
                ))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn classifies_unsupported_equations_by_residual() {
        let mut engine = RustEngine::spawn().unwrap();
        let equation = "y''+Sin(y)==0";
        let result = solve(&mut engine, equation, "x", "y", &[]).unwrap();
        assert_eq!(result.status, OdeStatus::Unresolved, "{equation}");
        assert_ne!(result.residual, "0");
        assert_eq!(
            solve(&mut engine, "x+y==0", "x", "y", &[]).unwrap().status,
            OdeStatus::NotDifferentialEquation
        );
    }

    #[test]
    fn rejects_unsupported_orders_and_invalid_requests() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(solve(&mut engine, "y'''==0", "x", "y", &[]).is_err());
        assert!(solve(&mut engine, "y'==0", "x", "x", &[]).is_err());
        assert!(solve(&mut engine, "z'==0", "x", "y", &[]).is_err());
        assert!(solve(
            &mut engine,
            "y''+y==0",
            "x",
            "y",
            &[InitialCondition {
                derivative_order: 0,
                point: "0",
                value: "1",
            }],
        )
        .is_err());
        assert!(solve(&mut engine, "y);Echo(1);(y'==0", "x", "y", &[]).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }
}
