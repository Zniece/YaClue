//! Structured API for algebraic equations and systems.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, strip_tex_delimiters, validate_expression, validate_symbol,
};
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
    Unknown,
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
}

pub fn solve(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<SolveResult, EngineError> {
    if equations.is_empty() {
        return Err(EngineError::Eval("至少需要一个方程".into()));
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
            return Err(EngineError::Eval("方程中没有可求解变量".into()));
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
        return Err(EngineError::Eval("求解变量不能重复".into()));
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
        return Err(EngineError::Eval("Solve 拒绝了求解变量或参数类型".into()));
    }
    let raw = raw_expr.to_string();
    let tex = engine
        .eval(&raw)
        .map(|result| strip_tex_delimiters(&result.tex))
        .unwrap_or_else(|_| raw.clone());
    if failed {
        return Ok(SolveResult {
            status: SolveStatus::Unresolved,
            solutions: vec![],
            raw,
            tex,
            variables: variables.iter().map(|value| (*value).to_string()).collect(),
            variable_source,
            parameters: vec![],
            completeness: SolveCompleteness::Unknown,
        });
    }

    let mut solutions = parse_solutions(&raw_expr, scalar)?;
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
    let completeness = if parameters.is_empty() {
        SolveCompleteness::Complete
    } else {
        SolveCompleteness::Parametric
    };
    Ok(SolveResult {
        status,
        solutions,
        raw,
        tex,
        variables: variables.iter().map(|value| (*value).to_string()).collect(),
        variable_source,
        parameters,
        completeness,
    })
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
}
