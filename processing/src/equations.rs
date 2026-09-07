//! Structured API for algebraic equations and systems.

use crate::conditions::{unpack_conditional, Condition};
use crate::engine::{Engine, EngineError, Expr};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus { Solved, NoSolution, Infinite, Unresolved }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assignment { pub variable: String, pub value: String }

#[derive(Debug, Clone, Serialize)]
pub struct SolveResult {
    pub status: SolveStatus,
    /// Alternative solution sets. Each inner vector is one simultaneous solution.
    pub solutions: Vec<Vec<Assignment>>,
    /// Facts used to justify the returned solution branch.
    pub conditions: Vec<Condition>,
    pub raw: String,
    pub tex: String,
}

pub fn solve(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<SolveResult, EngineError> {
    if equations.is_empty() { return Err(EngineError::Eval("至少需要一个方程".into())); }
    if variables.is_empty() { return Err(EngineError::Eval("至少需要一个求解变量".into())); }
    for equation in equations { validate_expression(equation)?; }
    for variable in variables { validate_variable(variable)?; }
    let mut unique = variables.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != variables.len() {
        return Err(EngineError::Eval("求解变量不能重复".into()));
    }

    let scalar = equations.len() == 1 && variables.len() == 1;
    let solve_call = if scalar {
        format!("SolveConditional({}, {})", equations[0], variables[0])
    } else {
        format!("Solve({{{}}}, {{{}}})", equations.join(","), variables.join(","))
    };
    // Solve returns {} both for valid no-solution cases and unsupported
    // equations. Capture its global status flags in the same request.
    let command = format!(
        "[ClearErrors(); Local(solution,failed,typeError); solution:={solve_call}; failed:=IsError(\"Solve'Fails\"); typeError:=IsError(\"Solve'TypeError\"); ClearErrors(); {{solution,failed,typeError}};]"
    );
    let wrapper = engine.eval(&command)?.expr;
    let (raw_expr, failed, type_error) = parse_wrapper(wrapper)?;
    if type_error {
        return Err(EngineError::Eval("Solve 拒绝了求解变量或参数类型".into()));
    }
    let (raw_expr, conditions) = unpack_conditional(raw_expr)?;
    let raw = raw_expr.to_string();
    let tex = engine.eval(&raw)
        .map(|result| strip_dollars(&result.tex))
        .unwrap_or_else(|_| raw.clone());
    if failed {
        return Ok(SolveResult {
            status: SolveStatus::Unresolved,
            solutions: vec![],
            conditions,
            raw,
            tex,
        });
    }

    let mut solutions = parse_solutions(&raw_expr, scalar)?;
    let infinite = solutions.iter().flatten().any(|assignment| {
        assignment.variable == assignment.value
            && variables.iter().any(|variable| *variable == assignment.variable)
    });
    if infinite { solutions.clear(); }
    let status = if infinite {
        SolveStatus::Infinite
    } else if solutions.is_empty() {
        SolveStatus::NoSolution
    } else {
        SolveStatus::Solved
    };
    Ok(SolveResult {
        status,
        solutions,
        conditions,
        raw,
        tex,
    })
}

fn parse_wrapper(wrapper: Expr) -> Result<(Expr, bool, bool), EngineError> {
    match wrapper {
        Expr::Call { head, mut args } if head == "List" && args.len() == 3 => {
            let type_error = bool_expr(args.pop().unwrap())?;
            let failed = bool_expr(args.pop().unwrap())?;
            Ok((args.pop().unwrap(), failed, type_error))
        }
        other => Err(EngineError::Parse(format!("Solve 包装结果形态异常: {other}"))),
    }
}

fn bool_expr(expr: Expr) -> Result<bool, EngineError> {
    match expr {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        other => Err(EngineError::Parse(format!("Solve 状态标志不是布尔值: {other}"))),
    }
}

fn parse_solutions(expr: &Expr, scalar: bool) -> Result<Vec<Vec<Assignment>>, EngineError> {
    let items = list_items(expr)?;
    if scalar {
        items.iter().map(|item| Ok(vec![parse_assignment(item)?])).collect()
    } else {
        items.iter()
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
            Ok(Assignment { variable: args[0].to_string(), value: args[1].to_string() })
        }
        other => Err(EngineError::Parse(format!("Solve 解不是等式: {other}"))),
    }
}

fn validate_expression(input: &str) -> Result<(), EngineError> {
    let input = input.trim();
    if input.is_empty() || input.chars().any(|c| matches!(c, ';' | '\n' | '\r' | ':' | '"')) {
        return Err(EngineError::Eval("方程为空或包含不允许的字符".into()));
    }
    for (open, close) in [('(', ')'), ('{', '}'), ('[', ']')] {
        if input.chars().filter(|&c| c == open).count() != input.chars().filter(|&c| c == close).count() {
            return Err(EngineError::Eval("方程括号不匹配".into()));
        }
    }
    Ok(())
}

fn validate_variable(variable: &str) -> Result<(), EngineError> {
    let mut chars = variable.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '\'')
    {
        return Err(EngineError::Eval(format!("无效求解变量: {variable}")));
    }
    Ok(())
}

fn strip_dollars(tex: &str) -> String {
    let tex = tex.trim();
    tex.strip_prefix('$').and_then(|value| value.strip_suffix('$')).unwrap_or(tex).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumptions::{assume, AssumptionFact};
    use crate::engine::RustEngine;

    #[test]
    fn solves_single_equations_and_parameterized_linear_forms() {
        let mut engine = RustEngine::spawn().unwrap();
        let quadratic = solve(&mut engine, &["x^2-3*x+2==0"], &["x"]).unwrap();
        assert_eq!(quadratic.status, SolveStatus::Solved);
        assert_eq!(quadratic.solutions.len(), 2);
        let mut roots: Vec<&str> = quadratic.solutions.iter().map(|set| set[0].value.as_str()).collect();
        roots.sort_unstable();
        assert_eq!(roots, ["1", "2"]);

        let parameterized = solve(&mut engine, &["a+x*y==z"], &["x"]).unwrap();
        assert_eq!(parameterized.status, SolveStatus::Solved);
        let value = &parameterized.solutions[0][0].value;
        assert_eq!(engine.eval(&format!("Simplify(({value})-((z-a)/y))")).unwrap().expr.to_string(), "0");
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
    fn reports_nonzero_assumptions_used_by_linear_solutions() {
        let mut engine = RustEngine::spawn().unwrap();
        assume(&mut engine, "a", AssumptionFact::NonZero).unwrap();
        let result = solve(&mut engine, &["a*x==b"], &["x"]).unwrap();
        assert_eq!(result.status, SolveStatus::Solved);
        assert_eq!(result.solutions[0][0].value, "(b / a)");
        assert_eq!(
            result.conditions,
            vec![Condition::Property {
                expression: "a".into(),
                fact: "NonZero".into(),
            }]
        );

        let mut engine = RustEngine::spawn().unwrap();
        assume(&mut engine, "a", AssumptionFact::Positive).unwrap();
        let compound = solve(&mut engine, &["(a+1)*x==b"], &["x"]).unwrap();
        assert_eq!(
            compound.conditions,
            vec![Condition::Property {
                expression: "(a + 1)".into(),
                fact: "NonZero".into(),
            }]
        );
    }

    #[test]
    fn distinguishes_no_solution_infinite_and_unresolved() {
        let mut engine = RustEngine::spawn().unwrap();
        assert_eq!(solve(&mut engine, &["Sqrt(x)==-1"], &["x"]).unwrap().status, SolveStatus::NoSolution);
        assert_eq!(solve(&mut engine, &["0==0"], &["x"]).unwrap().status, SolveStatus::Infinite);
        assert_eq!(solve(&mut engine, &["x^x==1"], &["x"]).unwrap().status, SolveStatus::Unresolved);
    }

    #[test]
    fn rejects_invalid_requests_without_poisoning_the_engine() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(solve(&mut engine, &[], &["x"]).is_err());
        assert!(solve(&mut engine, &["x==1"], &["x", "x"]).is_err());
        assert!(solve(&mut engine, &["x);Echo(1);(x==1"], &["x"]).is_err());
        assert_eq!(solve(&mut engine, &["x==1"], &["x"]).unwrap().status, SolveStatus::Solved);
    }
}
