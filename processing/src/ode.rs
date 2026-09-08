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

pub fn solve(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
) -> Result<OdeResult, EngineError> {
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
        return Ok(OdeResult {
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
        });
    }

    let canonical = to_canonical(equation, independent, dependent, order);
    let prefer_extension = contains_exact_power(equation, dependent, "2", "微分方程")?
        || contains_product_factor(equation, &format!("{dependent}'"), "微分方程")?
        || contains_ratio_symbols(equation, independent, dependent, "微分方程")?;
    let preferred = if prefer_extension {
        try_extension(engine, &canonical)?
    } else {
        None
    };
    let (mut candidates, mut residual, mut method) = if let Some((solutions, method)) = preferred {
        (solutions, Expr::Number("0".into()), method)
    } else {
        let upstream = engine.eval_expr(&format!(
            "[Local(sol,res); sol:=OdeSolve({canonical}); \
             res:=If(sol=True,{canonical},Simplify(OdeTest({canonical},sol))); \
             {{sol,res,Upstream}};]"
        ))?;
        let (solution, residual, method) = parse_wrapper(upstream)?;
        (vec![solution], residual, method)
    };
    if residual.to_string() != "0" && !prefer_extension {
        if let Some((solutions, extension_method)) = try_extension(engine, &canonical)? {
            candidates = solutions;
            residual = Expr::Number("0".into());
            method = extension_method;
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
    Ok(OdeResult {
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
    })
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

fn equation_order(equation: &str, dependent: &str) -> Result<u32, EngineError> {
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

fn to_canonical(equation: &str, independent: &str, dependent: &str, order: u32) -> String {
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
    result
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
                other => return Err(EngineError::Parse(format!("未知 ODE 求解方法: {other}"))),
            };
            let residual = args.pop().unwrap();
            Ok((args.pop().unwrap(), residual, method))
        }
        other => Err(EngineError::Parse(format!("ODE 包装结果形态异常: {other}"))),
    }
}

fn parse_extension(expr: Expr) -> Result<Option<(Vec<Expr>, OdeMethod)>, EngineError> {
    let Expr::Call { head, mut args } = expr else {
        return Err(EngineError::Parse("ODE 扩展结果不是列表".into()));
    };
    if head != "List" {
        return Err(EngineError::Parse("ODE 扩展结果不是列表".into()));
    }
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() != 2 {
        return Err(EngineError::Parse("ODE 扩展结果形态异常".into()));
    }
    let method = match args.pop().unwrap() {
        Expr::Symbol(value) if value == "Separable" => OdeMethod::Separable,
        Expr::Symbol(value) if value == "LinearFirstOrder" => OdeMethod::LinearFirstOrder,
        Expr::Symbol(value) if value == "Bernoulli" => OdeMethod::Bernoulli,
        Expr::Symbol(value) if value == "Exact" => OdeMethod::Exact,
        Expr::Symbol(value) if value == "Homogeneous" => OdeMethod::Homogeneous,
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
    Ok(Some((candidates, method)))
}

fn try_extension(
    engine: &mut dyn Engine,
    canonical: &str,
) -> Result<Option<(Vec<Expr>, OdeMethod)>, EngineError> {
    let extension = engine.eval_expr(&format!(
        "[Local(ext); \
         ext:=OdeExtSolveSeparable({canonical}); \
         if (Length(ext) != 2) [ext:=OdeExtSolveLinearFirstOrder({canonical});]; \
         if (Length(ext) != 2) [ext:=OdeExtSolveBernoulli({canonical});]; \
         if (Length(ext) != 2) [ext:=OdeExtSolveExact({canonical});]; \
         if (Length(ext) != 2) [ext:=OdeExtSolveHomogeneous({canonical});]; ext;]"
    ))?;
    let Some((raw_candidates, method)) = parse_extension(extension)? else {
        return Ok(None);
    };
    let mut verified = Vec::new();
    for candidate in raw_candidates {
        let valid = if method == OdeMethod::Exact {
            engine
                .eval_expr(&format!("OdeExtVerifyExact({canonical},{candidate})"))?
                .to_string()
                == "True"
        } else if method == OdeMethod::Homogeneous && is_implicit_solution(&candidate) {
            engine
                .eval_expr(&format!("OdeExtVerifyHomogeneous({canonical},{candidate})"))?
                .to_string()
                == "True"
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
    Ok((!verified.is_empty()).then_some((verified, method)))
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
        assert!(translated.solution.contains('u'));

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
    fn translates_homogeneous_equations_and_rejects_false_candidates() {
        let mut engine = RustEngine::spawn().unwrap();
        let translated = solve(&mut engine, "u'==(t+u)/t", "t", "u", &[]).unwrap();
        assert_eq!(translated.method, OdeMethod::Homogeneous);
        assert!(translated.solution.contains('t'));
        assert!(translated.solution.contains('u'));

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
