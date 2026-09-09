//! Unconstrained critical-point analysis for bivariate scalar functions.

use crate::engine::{Engine, EngineError, Expr};
use crate::equations::{self, Assignment, SolveCompleteness, SolveStatus};
use crate::input::{
    analyze_expression, strip_tex_delimiters, validate_expression, validate_symbol,
};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtremaStatus {
    Classified,
    NoCriticalPoints,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticalPointKind {
    LocalMinimum,
    LocalMaximum,
    Saddle,
    Degenerate,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize)]
pub struct CriticalPoint {
    pub coordinates: Vec<Assignment>,
    pub value: String,
    pub gradient: Vec<String>,
    pub gradient_verified: bool,
    pub hessian: Vec<Vec<String>>,
    pub hessian_determinant: String,
    pub kind: CriticalPointKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtremaResult {
    pub status: ExtremaStatus,
    pub expression: String,
    pub variables: [String; 2],
    pub gradient: Vec<String>,
    pub critical_points: Vec<CriticalPoint>,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtremaStepResult {
    pub result: ExtremaResult,
    pub steps: Vec<Step>,
}

pub fn analyze(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
) -> Result<ExtremaResult, EngineError> {
    validate_request(expression, x, y)?;
    analyze_internal(engine, expression, x, y, true)
}

pub fn analyze_steps(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
) -> Result<ExtremaStepResult, EngineError> {
    analyze_steps_with_verbosity(engine, expression, x, y, StepVerbosity::Detailed)
}

pub fn analyze_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    verbosity: StepVerbosity,
) -> Result<ExtremaStepResult, EngineError> {
    validate_request(expression, x, y)?;
    let mut result = analyze_internal(engine, expression, x, y, false)?;
    let gradient_equations = format!("{{{}==0,{}==0}}", result.gradient[0], result.gradient[1]);
    let mut events = vec![StepEvent::new(
        "extrema-gradient",
        &gradient_equations,
        "令两个一阶偏导数为零以寻找临界点。",
        StepImportance::Key,
    )];
    for point in &result.critical_points {
        let coordinates = assignments_expression(&point.coordinates);
        events.push(StepEvent::new(
            "extrema-critical-point",
            &coordinates,
            "得到一个经过梯度零检验的临界点。",
            StepImportance::Normal,
        ));
        events.push(StepEvent::new(
            "extrema-hessian",
            &matrix_expression(&point.hessian),
            &format!(
                "在该点计算 Hessian；其行列式为 {}。",
                point.hessian_determinant
            ),
            StepImportance::Routine,
        ));
        events.push(StepEvent::new(
            "extrema-classify",
            &coordinates,
            classification_explanation(point.kind),
            StepImportance::Key,
        ));
    }
    let final_expression = if result.critical_points.is_empty() {
        "List()".into()
    } else {
        format!(
            "{{{}}}",
            result
                .critical_points
                .iter()
                .map(|point| assignments_expression(&point.coordinates))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    events.push(StepEvent::new(
        "extrema-result",
        &final_expression,
        match result.status {
            ExtremaStatus::Classified => "得到所有已验证临界点的二阶分类。",
            ExtremaStatus::NoCriticalPoints => "没有有限临界点。",
            ExtremaStatus::Unresolved => "当前方程求解能力未能完整确定临界点。",
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(ExtremaStepResult { result, steps })
}

fn analyze_internal(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    render_value: bool,
) -> Result<ExtremaResult, EngineError> {
    let gradient_expr = engine.eval_expr(&format!(
        "{{Deriv({x})({expression}),Deriv({y})({expression})}}"
    ))?;
    let gradient = list(&gradient_expr, "梯度")?
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let equations = [format!("{}==0", gradient[0]), format!("{}==0", gradient[1])];
    let equation_refs = equations.iter().map(String::as_str).collect::<Vec<_>>();
    let solved = equations::solve(engine, &equation_refs, &[x, y]);
    let (resolution, solutions) = match solved {
        Ok(solved) if complete_solution(&solved) => {
            (CandidateResolution::Complete, solved.solutions)
        }
        Ok(_) if gradient_is_decoupled(&gradient, x, y)? => {
            solve_decoupled_gradient(engine, &equations, x, y)?
        }
        Ok(_) | Err(EngineError::Eval(_)) => (CandidateResolution::Unresolved, Vec::new()),
        Err(error) => return Err(error),
    };
    let critical_points = if resolution == CandidateResolution::Complete {
        classify_points(engine, expression, x, y, &gradient, &solutions)?
    } else {
        Vec::new()
    };
    let status = match resolution {
        CandidateResolution::None => ExtremaStatus::NoCriticalPoints,
        CandidateResolution::Complete if !critical_points.is_empty() => ExtremaStatus::Classified,
        _ => ExtremaStatus::Unresolved,
    };
    let display = if critical_points.is_empty() {
        "List()".into()
    } else {
        format!(
            "{{{}}}",
            critical_points
                .iter()
                .map(|point| assignments_expression(&point.coordinates))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let tex = if render_value {
        strip_tex_delimiters(&engine.render_tex_batch(&[display])?[0])
    } else {
        String::new()
    };
    Ok(ExtremaResult {
        status,
        expression: expression.into(),
        variables: [x.into(), y.into()],
        gradient,
        critical_points,
        tex,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateResolution {
    Complete,
    None,
    Unresolved,
}

fn complete_solution(result: &equations::SolveResult) -> bool {
    result.status == SolveStatus::Solved
        && result.completeness == SolveCompleteness::Complete
        && result.parameters.is_empty()
}

fn gradient_is_decoupled(gradient: &[String], x: &str, y: &str) -> Result<bool, EngineError> {
    let first = analyze_expression(&gradient[0], "第一个梯度分量")?;
    let second = analyze_expression(&gradient[1], "第二个梯度分量")?;
    Ok(!first.symbols.iter().any(|symbol| symbol == y)
        && !second.symbols.iter().any(|symbol| symbol == x))
}

fn solve_decoupled_gradient(
    engine: &mut dyn Engine,
    equations: &[String; 2],
    x: &str,
    y: &str,
) -> Result<(CandidateResolution, Vec<Vec<Assignment>>), EngineError> {
    let x_solutions = equations::solve(engine, &[&equations[0]], &[x])?;
    let y_solutions = equations::solve(engine, &[&equations[1]], &[y])?;
    if matches!(
        x_solutions.status,
        SolveStatus::Unresolved | SolveStatus::Infinite
    ) || matches!(
        y_solutions.status,
        SolveStatus::Unresolved | SolveStatus::Infinite
    ) || (x_solutions.status == SolveStatus::Solved && !complete_solution(&x_solutions))
        || (y_solutions.status == SolveStatus::Solved && !complete_solution(&y_solutions))
    {
        return Ok((CandidateResolution::Unresolved, Vec::new()));
    }
    if x_solutions.status == SolveStatus::NoSolution
        || y_solutions.status == SolveStatus::NoSolution
    {
        return Ok((CandidateResolution::None, Vec::new()));
    }
    let mut combined = Vec::new();
    let mut seen = BTreeSet::new();
    for x_solution in x_solutions.solutions {
        for y_solution in &y_solutions.solutions {
            let mut point = x_solution.clone();
            point.extend(y_solution.clone());
            let key = point
                .iter()
                .map(|item| format!("{}={}", item.variable, item.value))
                .collect::<Vec<_>>()
                .join(";");
            if seen.insert(key) {
                combined.push(point);
            }
        }
    }
    Ok((CandidateResolution::Complete, combined))
}

fn classify_points(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    gradient: &[String],
    solutions: &[Vec<Assignment>],
) -> Result<Vec<CriticalPoint>, EngineError> {
    let solutions = solutions
        .iter()
        .filter(|solution| {
            solution.len() == 2
                && solution.iter().any(|item| item.variable == x)
                && solution.iter().any(|item| item.variable == y)
        })
        .collect::<Vec<_>>();
    if solutions.is_empty() {
        return Ok(Vec::new());
    }
    let hxx = format!("Deriv({x},2)({expression})");
    let hxy = format!("Deriv({y})(Deriv({x})({expression}))");
    let hyx = format!("Deriv({x})(Deriv({y})({expression}))");
    let hyy = format!("Deriv({y},2)({expression})");
    let records = solutions
        .iter()
        .map(|solution| {
            let substitute = |value: &str| substitute_all(value, solution);
            let gx = substitute(&gradient[0]);
            let gy = substitute(&gradient[1]);
            let at_hxx = substitute(&hxx);
            let at_hxy = substitute(&hxy);
            let at_hyx = substitute(&hyx);
            let at_hyy = substitute(&hyy);
            format!(
                "[Local(gx,gy,a,b,c,d,det);gx:=Simplify({gx});gy:=Simplify({gy});a:=Simplify({at_hxx});b:=Simplify({at_hxy});c:=Simplify({at_hyx});d:=Simplify({at_hyy});det:=Simplify(a*d-b*c);{{Simplify({}),gx,gy,a,b,c,d,det,IsZero(gx) And IsZero(gy),IsPositiveNumber(det),IsNegativeNumber(det),IsPositiveNumber(a),IsNegativeNumber(a),IsZero(det)}};]",
                substitute(expression)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let evaluated = engine.eval_expr(&format!("{{{records}}}"))?;
    let evaluated = list(&evaluated, "临界点分类结果")?;
    if evaluated.len() != solutions.len() {
        return Err(EngineError::Parse("临界点分类结果数量异常".into()));
    }
    solutions
        .into_iter()
        .zip(evaluated)
        .map(|(solution, record)| parse_point(solution, record))
        .collect()
}

fn parse_point(solution: &[Assignment], record: &Expr) -> Result<CriticalPoint, EngineError> {
    let fields = list(record, "临界点分类记录")?;
    if fields.len() != 14 {
        return Err(EngineError::Parse("临界点分类记录形态异常".into()));
    }
    let gradient_verified = boolean(&fields[8], "梯度零证书")?;
    if !gradient_verified {
        return Err(EngineError::Parse(format!(
            "临界点候选未通过梯度零证书: {}",
            assignments_expression(solution)
        )));
    }
    let det_positive = boolean(&fields[9], "Hessian 行列式正性")?;
    let det_negative = boolean(&fields[10], "Hessian 行列式负性")?;
    let hxx_positive = boolean(&fields[11], "Hessian 首项正性")?;
    let hxx_negative = boolean(&fields[12], "Hessian 首项负性")?;
    let det_zero = boolean(&fields[13], "Hessian 行列式零性")?;
    let kind = if det_positive && hxx_positive {
        CriticalPointKind::LocalMinimum
    } else if det_positive && hxx_negative {
        CriticalPointKind::LocalMaximum
    } else if det_negative {
        CriticalPointKind::Saddle
    } else if det_zero {
        CriticalPointKind::Degenerate
    } else {
        CriticalPointKind::Inconclusive
    };
    Ok(CriticalPoint {
        coordinates: solution.to_vec(),
        value: fields[0].to_string(),
        gradient: vec![fields[1].to_string(), fields[2].to_string()],
        gradient_verified,
        hessian: vec![
            vec![fields[3].to_string(), fields[4].to_string()],
            vec![fields[5].to_string(), fields[6].to_string()],
        ],
        hessian_determinant: fields[7].to_string(),
        kind,
    })
}

fn validate_request(expression: &str, x: &str, y: &str) -> Result<(), EngineError> {
    validate_expression(expression, "多元极值表达式")?;
    validate_symbol(x, "第一个极值变量")?;
    validate_symbol(y, "第二个极值变量")?;
    if x == y {
        return Err(EngineError::InvalidInput("极值分析变量不能重复".into()));
    }
    Ok(())
}

fn substitute_all(expression: &str, solution: &[Assignment]) -> String {
    solution
        .iter()
        .fold(expression.to_string(), |current, item| {
            format!(
                "Eval(ApplyPure(\"Subst\",{{{},{},{current}}}))",
                item.variable, item.value
            )
        })
}

fn assignments_expression(assignments: &[Assignment]) -> String {
    format!(
        "{{{}}}",
        assignments
            .iter()
            .map(|item| format!("{}=={}", item.variable, item.value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn matrix_expression(matrix: &[Vec<String>]) -> String {
    format!(
        "{{{}}}",
        matrix
            .iter()
            .map(|row| format!("{{{}}}", row.join(",")))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn classification_explanation(kind: CriticalPointKind) -> &'static str {
    match kind {
        CriticalPointKind::LocalMinimum => "Hessian 行列式为正且首个主子式为正，因此是局部极小点。",
        CriticalPointKind::LocalMaximum => "Hessian 行列式为正且首个主子式为负，因此是局部极大点。",
        CriticalPointKind::Saddle => "Hessian 行列式为负，因此是鞍点。",
        CriticalPointKind::Degenerate => "Hessian 行列式为零，二阶判别法无法确定极值类型。",
        CriticalPointKind::Inconclusive => "Hessian 的符号条件不足，当前二阶判别无法分类。",
    }
}

fn list<'a>(expression: &'a Expr, label: &str) -> Result<&'a [Expr], EngineError> {
    match expression {
        Expr::Call { head, args } if head == "List" => Ok(args),
        other => Err(EngineError::Parse(format!("{label}不是列表: {other}"))),
    }
}

fn boolean(expression: &Expr, label: &str) -> Result<bool, EngineError> {
    match expression {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        other => Err(EngineError::Parse(format!("{label}不是布尔值: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn classifies_minimum_maximum_saddle_and_degenerate_points() {
        let mut engine = RustEngine::spawn().unwrap();
        for (expression, expected) in [
            ("x^2+y^2", CriticalPointKind::LocalMinimum),
            ("x^2+x*y+y^2", CriticalPointKind::LocalMinimum),
            ("-x^2-y^2", CriticalPointKind::LocalMaximum),
            ("x^2-y^2", CriticalPointKind::Saddle),
            ("x^4+y^4", CriticalPointKind::Degenerate),
        ] {
            let result = analyze(&mut engine, expression, "x", "y").unwrap();
            assert_eq!(result.status, ExtremaStatus::Classified, "{expression}");
            assert_eq!(result.critical_points.len(), 1, "{expression}");
            let point = &result.critical_points[0];
            assert_eq!(point.kind, expected, "{expression}");
            assert!(point.gradient_verified);
            assert_eq!(point.gradient, ["0", "0"]);
        }
    }

    #[test]
    fn classifies_multiple_points_and_emits_filtered_steps() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = analyze_steps(&mut engine, "x^3-3*x+y^2", "x", "y").unwrap();
        assert_eq!(result.result.critical_points.len(), 2);
        assert!(result
            .result
            .critical_points
            .iter()
            .any(|point| point.kind == CriticalPointKind::LocalMinimum));
        assert!(result
            .result
            .critical_points
            .iter()
            .any(|point| point.kind == CriticalPointKind::Saddle));
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "extrema-hessian"));

        let concise =
            analyze_steps_with_verbosity(&mut engine, "x^2+y^2", "x", "y", StepVerbosity::Concise)
                .unwrap();
        assert!(concise
            .steps
            .iter()
            .all(|step| step.importance == StepImportance::Key));
    }

    #[test]
    fn reports_no_points_unresolved_and_invalid_requests() {
        let mut engine = RustEngine::spawn().unwrap();
        let none = analyze(&mut engine, "Exp(x)+Exp(y)", "x", "y").unwrap();
        assert_eq!(none.status, ExtremaStatus::NoCriticalPoints);
        assert!(none.critical_points.is_empty());

        let unresolved = analyze(&mut engine, "x^x+y^y", "x", "y").unwrap();
        assert_eq!(unresolved.status, ExtremaStatus::Unresolved);
        assert!(unresolved.critical_points.is_empty());

        let coupled_family = analyze(&mut engine, "(x*y-1)^2", "x", "y").unwrap();
        assert_eq!(coupled_family.status, ExtremaStatus::Unresolved);
        assert!(coupled_family.critical_points.is_empty());

        assert!(analyze(&mut engine, "x+y", "x", "x").is_err());
        assert!(analyze(&mut engine, "x);Echo(1);(x", "x", "y").is_err());
    }
}
