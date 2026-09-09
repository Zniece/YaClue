//! Structured API for algebraic equations and systems.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, direct_function_equation, strip_tex_delimiters, validate_expression,
    validate_symbol,
};
use crate::steps::{Step, StepImportance, StepVerbosity};
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

struct EquationEvent {
    rule: &'static str,
    expr: String,
    why: String,
    importance: StepImportance,
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
    let diagnostic = engine.eval_expr(&format!(
        "[Local(p,d,f); p:=NormalForm({residual}); If(CanBeUni({variable},p), [d:=Degree(p,{variable}); f:=If(d<=8,Factor(p),p); {{True,p,d,f,{{{checks}}}}};], {{False,p,0,p,{{{checks}}}}});]"
    ))?;
    let Expr::Call { head, args } = diagnostic else {
        return Err(EngineError::Parse("方程步骤诊断不是列表".into()));
    };
    if head != "List" || args.len() != 5 {
        return Err(EngineError::Parse("方程步骤诊断形态异常".into()));
    }
    let polynomial = matches!(&args[0], Expr::Symbol(value) if value == "True");
    let normalized = args[1].to_string();
    let degree = match &args[2] {
        Expr::Number(value) => value.parse::<usize>().ok(),
        _ => None,
    };
    let factored = args[3].to_string();
    let verified = match &args[4] {
        Expr::Call { head, args } if head == "List" => args
            .iter()
            .map(|value| matches!(value, Expr::Symbol(symbol) if symbol == "True"))
            .collect::<Vec<_>>(),
        _ => return Err(EngineError::Parse("候选解残差证书不是列表".into())),
    };

    let mut events = vec![EquationEvent {
        rule: "equation-start",
        expr: equation.to_string(),
        why: format!("建立关于 {variable} 的方程。"),
        importance: StepImportance::Routine,
    }];
    if polynomial {
        events.push(EquationEvent {
            rule: "equation-normalize",
            expr: format!("{normalized}==0"),
            why: "将方程移到一边并整理为多项式标准形。".into(),
            importance: StepImportance::Normal,
        });
        match degree {
            Some(1) => events.push(EquationEvent {
                rule: "equation-linear",
                expr: result.raw.clone(),
                why: format!("合并同类项并解出 {variable}。"),
                importance: StepImportance::Normal,
            }),
            Some(2) => events.push(EquationEvent {
                rule: "equation-quadratic",
                expr: format!("{normalized}==0"),
                why: "使用二次方程求根公式求出各个分支。".into(),
                importance: StepImportance::Normal,
            }),
            Some(3..=8) if factored != normalized => events.push(EquationEvent {
                rule: "equation-factor",
                expr: format!("{factored}==0"),
                why: "因式分解后令每个因式分别为零。".into(),
                importance: StepImportance::Key,
            }),
            _ => {}
        }
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
                events.push(EquationEvent {
                    rule: "equation-branch",
                    expr: format!("{}=={}", solution[0].variable, solution[0].value),
                    why: "得到一个候选解分支。".into(),
                    importance: StepImportance::Normal,
                });
                events.push(EquationEvent {
                    rule: "equation-verify",
                    expr: "0".into(),
                    why: "将该候选解代回原方程，残差为零。".into(),
                    importance: StepImportance::Routine,
                });
            }
        }
    }
    events.push(EquationEvent {
        rule: "equation-result",
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

    let last = events.len() - 1;
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
    let expressions = events
        .iter()
        .map(|event| event.expr.clone())
        .collect::<Vec<_>>();
    let tex = engine.render_tex_batch(&expressions)?;
    Ok(events
        .into_iter()
        .zip(tex)
        .map(|(event, tex)| Step {
            rule: event.rule.into(),
            expr: event.expr,
            why: event.why,
            tex: strip_tex_delimiters(&tex),
            importance: event.importance,
        })
        .collect())
}

pub fn solve(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
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
        return solve_rectangular(engine, equations, &variables, variable_source);
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
    let command = format!(
        "[ClearErrors(); Local(solution,failed,typeError); solution:={solve_call}; failed:=IsError(\"Solve'Fails\"); typeError:=IsError(\"Solve'TypeError\"); ClearErrors(); {{solution,failed,typeError}};]"
    );
    let wrapper = engine.eval(&command)?.expr;
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

    let mut solutions = parse_solutions(&raw_expr, scalar)?;
    if !scalar {
        solutions.retain(|solution| candidate_is_finite(solution));
    }
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
        SolveStatus::NoSolution
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
    let mut completeness = if parameters.is_empty() {
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
        let mut result = solve(engine, &equation_subset, &variable_subset)?;
        match result.status {
            SolveStatus::Solved => {
                saw_complete_subproblem |= result.completeness == SolveCompleteness::Complete;
                let mut verified = Vec::new();
                for solution in result.solutions {
                    if candidate_is_finite(&solution)
                        && candidate_satisfies(engine, &solution, equations)?
                    {
                        verified.push(solution);
                    }
                }
                result.solutions = verified;
                if result.solutions.is_empty() {
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

fn candidate_is_finite(solution: &[Assignment]) -> bool {
    solution.iter().all(|assignment| {
        !assignment.value.contains("Infinity") && !assignment.value.contains("Undefined")
    })
}

fn candidate_satisfies(
    engine: &mut dyn Engine,
    solution: &[Assignment],
    equations: &[&str],
) -> Result<bool, EngineError> {
    let checks: Vec<_> = equations
        .iter()
        .map(|equation| {
            let mut substituted = equation
                .split_once("==")
                .map(|(left, right)| format!("({left})-({right})"))
                .unwrap_or_else(|| (*equation).to_string());
            for assignment in solution {
                substituted = format!(
                    "Eval(ApplyPure(\"Subst\",{{{},{},{substituted}}}))",
                    assignment.variable, assignment.value
                );
            }
            format!("IsZero(Simplify({substituted}))")
        })
        .collect();
    let result = engine.eval_expr(&format!("{{{}}}", checks.join(",")))?;
    match result {
        Expr::Call { head, args } if head == "List" => Ok(args
            .iter()
            .all(|value| matches!(value, Expr::Symbol(symbol) if symbol == "True"))),
        _ => Ok(false),
    }
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
            .map(|item| Ok(vec![parse_assignment(item)?]))
            .collect()
    } else {
        items
            .iter()
            .map(|solution| list_items(solution)?.iter().map(parse_assignment).collect())
            .collect()
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
        assert_eq!(explicit_subset.completeness, SolveCompleteness::Parametric);
        assert_eq!(explicit_subset.parameters, ["y"]);

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
                    assert!(candidate_satisfies(&mut engine, &instantiated, &[equation]).unwrap());
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
}
