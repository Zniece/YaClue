//! Structured finite-sum and infinite-series APIs.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, strip_tex_delimiters, validate_expression, validate_symbol,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SumStatus {
    Evaluated,
    Undefined,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FiniteSumResult {
    pub variable: String,
    pub from: String,
    pub to: String,
    pub term: String,
    pub value: String,
    pub status: SumStatus,
    pub tex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesStatus {
    AbsolutelyConvergent,
    Divergent,
    ConditionallyConvergent,
    /// Convergence depends on the returned conditions.
    Conditional,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceMethod {
    Geometric,
    PSeries,
    Alternating,
    Comparison,
    Term,
    Ratio,
    Root,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InfiniteSeriesResult {
    pub variable: String,
    pub from: String,
    pub term: String,
    pub status: SeriesStatus,
    pub method: ConvergenceMethod,
    pub test_value: Option<String>,
    pub conditions: Vec<String>,
    /// A closed form is exposed only when convergence is established.
    pub value: Option<String>,
    pub tex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSeriesStatus {
    Convergent,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerSeriesResult {
    pub index: String,
    pub variable: String,
    pub center: String,
    pub coefficient: String,
    pub status: PowerSeriesStatus,
    pub method: ConvergenceMethod,
    pub radius: Option<String>,
    pub left_endpoint: Option<String>,
    pub right_endpoint: Option<String>,
    pub left_status: Option<SeriesStatus>,
    pub right_status: Option<SeriesStatus>,
    pub left_included: bool,
    pub right_included: bool,
    pub tex: String,
}

pub fn finite_sum(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
    to: &str,
) -> Result<FiniteSumResult, EngineError> {
    validate_request(term, variable, from)?;
    validate_expression(to, "求和上限")?;
    let evaluated = engine.eval(&format!("Sum({variable},{from},{to},{term})"))?;
    let value = evaluated.expr.to_string();
    let status = if matches!(&evaluated.expr, Expr::Symbol(symbol) if symbol == "Undefined") {
        SumStatus::Undefined
    } else if matches!(&evaluated.expr, Expr::Call { head, .. } if head == "Sum") {
        SumStatus::Unresolved
    } else {
        SumStatus::Evaluated
    };
    Ok(FiniteSumResult {
        variable: variable.into(),
        from: from.into(),
        to: to.into(),
        term: term.into(),
        value,
        status,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

pub fn infinite_series(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
) -> Result<InfiniteSeriesResult, EngineError> {
    validate_request(term, variable, from)?;
    if from.parse::<i64>().is_err() {
        return Err(EngineError::InvalidInput(
            "无穷级数下限必须是显式整数".into(),
        ));
    }
    let evaluated = engine.eval(&format!("SeriesConvergence({variable},{from},{term})"))?;
    let analysis = list(&evaluated.expr, "收敛分析")?;
    if analysis.len() != 5 {
        return Err(EngineError::Parse("收敛分析字段数量错误".into()));
    }
    let status = parse_status(&analysis[0])?;
    let method = parse_method(&analysis[1])?;
    let test_value = (!matches!(&analysis[2], Expr::Symbol(value) if value == "Undefined"))
        .then(|| analysis[2].to_string());
    let conditions = list(&analysis[3], "收敛条件")?
        .iter()
        .map(ToString::to_string)
        .collect();
    let value = matches!(
        status,
        SeriesStatus::AbsolutelyConvergent | SeriesStatus::ConditionallyConvergent
    )
    .then(|| analysis[4].to_string())
    .filter(|value| value != "Undefined");
    Ok(InfiniteSeriesResult {
        variable: variable.into(),
        from: from.into(),
        term: term.into(),
        status,
        method,
        test_value,
        conditions,
        value,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

/// Analyze `Sum(coefficient * (variable-center)^index)`.
pub fn power_series(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
) -> Result<PowerSeriesResult, EngineError> {
    let coefficient_analysis = analyze_expression(coefficient, "幂级数系数")?;
    validate_symbol(index, "幂级数指标")?;
    validate_symbol(variable, "幂级数变量")?;
    if index == variable {
        return Err(EngineError::InvalidInput(
            "幂级数指标与展开变量必须不同".into(),
        ));
    }
    if coefficient_analysis
        .symbols
        .iter()
        .any(|symbol| symbol == variable)
    {
        return Err(EngineError::InvalidInput(
            "幂级数系数不能包含展开变量".into(),
        ));
    }
    let center_analysis = analyze_expression(center, "幂级数中心")?;
    if center_analysis
        .symbols
        .iter()
        .any(|symbol| symbol == index || symbol == variable)
    {
        return Err(EngineError::InvalidInput(
            "幂级数中心不能依赖指标或展开变量".into(),
        ));
    }
    let evaluated = engine.eval(&format!(
        "PowerSeriesConvergence({index},{center},{coefficient})"
    ))?;
    let fields = list(&evaluated.expr, "幂级数分析")?;
    if fields.len() != 9 {
        return Err(EngineError::Parse("幂级数分析字段数量错误".into()));
    }
    let status = match string(&fields[0], "幂级数状态")? {
        "convergent" => PowerSeriesStatus::Convergent,
        "inconclusive" => PowerSeriesStatus::Inconclusive,
        value => return Err(EngineError::Parse(format!("未知幂级数状态: {value}"))),
    };
    let optional = |expr: &Expr| {
        (!matches!(expr, Expr::Symbol(value) if value == "Undefined")).then(|| expr.to_string())
    };
    Ok(PowerSeriesResult {
        index: index.into(),
        variable: variable.into(),
        center: center.into(),
        coefficient: coefficient.into(),
        status,
        method: parse_method(&fields[1])?,
        radius: optional(&fields[2]),
        left_endpoint: optional(&fields[3]),
        right_endpoint: optional(&fields[4]),
        left_status: parse_optional_status(&fields[5])?,
        right_status: parse_optional_status(&fields[6])?,
        left_included: parse_bool(&fields[7], "左端点包含状态")?,
        right_included: parse_bool(&fields[8], "右端点包含状态")?,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

fn validate_request(term: &str, variable: &str, from: &str) -> Result<(), EngineError> {
    validate_expression(term, "求和项")?;
    validate_symbol(variable, "求和变量")?;
    validate_expression(from, "求和下限")
}

fn list<'a>(expr: &'a Expr, label: &str) -> Result<&'a [Expr], EngineError> {
    match expr {
        Expr::Call { head, args } if head == "List" => Ok(args),
        _ => Err(EngineError::Parse(format!("{label} 不是列表"))),
    }
}

fn string<'a>(expr: &'a Expr, label: &str) -> Result<&'a str, EngineError> {
    match expr {
        Expr::Symbol(value) => Ok(value.trim_matches('"')),
        _ => Err(EngineError::Parse(format!("{label} 不是字符串"))),
    }
}

fn parse_status(expr: &Expr) -> Result<SeriesStatus, EngineError> {
    match string(expr, "收敛状态")? {
        "absolutely_convergent" => Ok(SeriesStatus::AbsolutelyConvergent),
        "divergent" => Ok(SeriesStatus::Divergent),
        "conditionally_convergent" => Ok(SeriesStatus::ConditionallyConvergent),
        "conditional" => Ok(SeriesStatus::Conditional),
        "inconclusive" => Ok(SeriesStatus::Inconclusive),
        value => Err(EngineError::Parse(format!("未知收敛状态: {value}"))),
    }
}

fn parse_method(expr: &Expr) -> Result<ConvergenceMethod, EngineError> {
    match string(expr, "判别方法")? {
        "geometric" => Ok(ConvergenceMethod::Geometric),
        "p_series" => Ok(ConvergenceMethod::PSeries),
        "alternating" => Ok(ConvergenceMethod::Alternating),
        "comparison" => Ok(ConvergenceMethod::Comparison),
        "term" => Ok(ConvergenceMethod::Term),
        "ratio" => Ok(ConvergenceMethod::Ratio),
        "root" => Ok(ConvergenceMethod::Root),
        "none" => Ok(ConvergenceMethod::None),
        value => Err(EngineError::Parse(format!("未知判别方法: {value}"))),
    }
}

fn parse_optional_status(expr: &Expr) -> Result<Option<SeriesStatus>, EngineError> {
    if string(expr, "端点状态")? == "none" {
        Ok(None)
    } else {
        parse_status(expr).map(Some)
    }
}

fn parse_bool(expr: &Expr, label: &str) -> Result<bool, EngineError> {
    match expr {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        _ => Err(EngineError::Parse(format!("{label} 不是布尔值"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn computes_finite_symbolic_and_numeric_sums() {
        let mut engine = RustEngine::spawn().unwrap();
        let symbolic = finite_sum(&mut engine, "k^2", "k", "1", "n").unwrap();
        assert_eq!(symbolic.status, SumStatus::Evaluated);
        assert!(symbolic.value.contains("n"));

        let numeric = finite_sum(&mut engine, "k", "k", "1", "100").unwrap();
        assert_eq!(numeric.value, "5050");
        assert!(!numeric.tex.is_empty());
    }

    #[test]
    fn classifies_geometric_and_p_series() {
        let mut engine = RustEngine::spawn().unwrap();
        let geometric = infinite_series(&mut engine, "(1/2)^k", "k", "0").unwrap();
        assert_eq!(geometric.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(geometric.method, ConvergenceMethod::Geometric);
        assert_eq!(geometric.test_value.as_deref(), Some("(1 / 2)"));
        assert_eq!(geometric.value.as_deref(), Some("2"));

        let p_series = infinite_series(&mut engine, "1/k^2", "k", "1").unwrap();
        assert_eq!(p_series.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(p_series.method, ConvergenceMethod::PSeries);
        assert_eq!(p_series.value.as_deref(), Some("((Pi ^ 2) / 6)"));
        assert_eq!(
            engine.eval("Zeta(2)").unwrap().expr.to_string(),
            "((Pi ^ 2) / 6)"
        );
        assert_eq!(
            engine
                .eval("Sum(k,1,Infinity,1/k^2)")
                .unwrap()
                .expr
                .to_string(),
            "((Pi ^ 2) / 6)"
        );

        let harmonic = infinite_series(&mut engine, "1/k", "k", "1").unwrap();
        assert_eq!(harmonic.status, SeriesStatus::Divergent);
        assert_eq!(harmonic.method, ConvergenceMethod::PSeries);
        assert_eq!(harmonic.value, None);
    }

    #[test]
    fn uses_ratio_test_and_reports_inconclusive_cases() {
        let mut engine = RustEngine::spawn().unwrap();
        let ratio = infinite_series(&mut engine, "k/2^k", "k", "1").unwrap();
        assert_eq!(ratio.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(ratio.method, ConvergenceMethod::Ratio);
        assert_eq!(ratio.test_value.as_deref(), Some("(1 / 2)"));

        let factorial = infinite_series(&mut engine, "1/k!", "k", "1").unwrap();
        assert_eq!(factorial.method, ConvergenceMethod::Ratio);

        let root = infinite_series(&mut engine, "(1/2)^(k^2)", "k", "1").unwrap();
        assert_eq!(root.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(root.method, ConvergenceMethod::Root);

        let alternating = infinite_series(&mut engine, "(-1)^k/k", "k", "1").unwrap();
        assert_eq!(alternating.status, SeriesStatus::ConditionallyConvergent);
    }

    #[test]
    fn returns_symbolic_geometric_conditions_and_rejects_bad_input() {
        let mut engine = RustEngine::spawn().unwrap();
        let symbolic = infinite_series(&mut engine, "r^k", "k", "0").unwrap();
        assert_eq!(symbolic.status, SeriesStatus::Conditional);
        assert_eq!(symbolic.method, ConvergenceMethod::Geometric);
        assert_eq!(symbolic.conditions.len(), 1);
        assert!(infinite_series(&mut engine, "1/k^2", "k", "n").is_err());
        assert_eq!(
            infinite_series(&mut engine, "1/k^2", "k", "0")
                .unwrap()
                .status,
            SeriesStatus::Inconclusive
        );
        assert!(finite_sum(&mut engine, "k);Echo(1);(", "k", "1", "2").is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }

    #[test]
    fn classifies_alternating_and_comparison_series() {
        let mut engine = RustEngine::spawn().unwrap();
        let alternating = infinite_series(&mut engine, "(-1)^(k-1)/k", "k", "1").unwrap();
        assert_eq!(alternating.status, SeriesStatus::ConditionallyConvergent);
        assert_eq!(alternating.method, ConvergenceMethod::Alternating);
        assert_eq!(alternating.value.as_deref(), Some("Ln(2)"));

        let absolute = infinite_series(&mut engine, "(-1)^k/k^2", "k", "1").unwrap();
        assert_eq!(absolute.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(absolute.method, ConvergenceMethod::Alternating);

        let comparison = infinite_series(&mut engine, "1/(k^2+1)", "k", "1").unwrap();
        assert_eq!(comparison.status, SeriesStatus::AbsolutelyConvergent);
        assert_eq!(comparison.method, ConvergenceMethod::Comparison);
        assert_eq!(comparison.test_value.as_deref(), Some("1"));

        let divergent = infinite_series(&mut engine, "1/(k+1)", "k", "1").unwrap();
        assert_eq!(divergent.status, SeriesStatus::Divergent);
        assert_eq!(divergent.method, ConvergenceMethod::Comparison);
    }

    #[test]
    fn returns_power_series_radius_and_endpoint_contracts() {
        let mut engine = RustEngine::spawn().unwrap();
        let harmonic = power_series(&mut engine, "1/k", "k", "x", "2").unwrap();
        assert_eq!(harmonic.status, PowerSeriesStatus::Convergent);
        assert_eq!(harmonic.radius.as_deref(), Some("1"));
        assert_eq!(harmonic.left_endpoint.as_deref(), Some("1"));
        assert_eq!(harmonic.right_endpoint.as_deref(), Some("3"));
        assert_eq!(
            harmonic.left_status,
            Some(SeriesStatus::ConditionallyConvergent)
        );
        assert_eq!(harmonic.right_status, Some(SeriesStatus::Divergent));
        assert!(harmonic.left_included);
        assert!(!harmonic.right_included);

        let square = power_series(&mut engine, "1/k^2", "k", "x", "0").unwrap();
        assert!(square.left_included && square.right_included);
        assert_eq!(
            square.right_status,
            Some(SeriesStatus::AbsolutelyConvergent)
        );

        let exponential = power_series(&mut engine, "1/3^k", "k", "x", "0").unwrap();
        assert_eq!(exponential.radius.as_deref(), Some("3"));
        assert!(!exponential.left_included && !exponential.right_included);

        let entire = power_series(&mut engine, "1/k!", "k", "x", "0").unwrap();
        assert_eq!(entire.radius.as_deref(), Some("Infinity"));
        assert_eq!(entire.left_status, None);
        assert!(power_series(&mut engine, "1/k", "x", "x", "0").is_err());
        assert!(power_series(&mut engine, "x/k", "k", "x", "0").is_err());
        assert!(power_series(&mut engine, "1/k", "k", "x", "x+1").is_err());
    }
}
