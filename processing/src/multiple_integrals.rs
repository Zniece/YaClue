//! Structured bounded iterated integrals.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, fresh_internal_symbols, render_one_tex, validate_expression,
    validate_symbol,
};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct IntegralBound<'a> {
    pub variable: &'a str,
    pub lower: &'a str,
    pub upper: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IteratedIntegralStatus {
    Evaluated,
    InnerUnresolved,
    OuterUnresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegralLayerResult {
    pub variable: String,
    pub lower: String,
    pub upper: String,
    pub integrand: String,
    pub value: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoubleIntegralResult {
    pub status: IteratedIntegralStatus,
    pub expression: String,
    pub inner: IntegralLayerResult,
    pub outer: Option<IntegralLayerResult>,
    pub value: String,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoubleIntegralStepResult {
    pub result: DoubleIntegralResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TripleIntegralStatus {
    Evaluated,
    InnerUnresolved,
    MiddleUnresolved,
    OuterUnresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct TripleIntegralResult {
    pub status: TripleIntegralStatus,
    pub expression: String,
    /// Reached layers in evaluation order: inner, middle, outer.
    pub layers: Vec<IntegralLayerResult>,
    pub value: String,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TripleIntegralStepResult {
    pub result: TripleIntegralResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy)]
pub struct PolarRegion<'a> {
    pub radial_lower: &'a str,
    pub radial_upper: &'a str,
    pub angle_lower: &'a str,
    pub angle_upper: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolarRegionKind {
    Disk,
    Annulus,
    Sector,
    AnnularSector,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolarIntegralResult {
    pub cartesian_expression: String,
    pub cartesian_variables: [String; 2],
    pub polar_variables: [String; 2],
    pub region_kind: PolarRegionKind,
    pub x_substitution: String,
    pub y_substitution: String,
    pub jacobian: String,
    pub jacobian_verified: bool,
    pub transformed_integrand: String,
    pub integral: DoubleIntegralResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolarIntegralStepResult {
    pub result: PolarIntegralResult,
    pub steps: Vec<Step>,
}

#[allow(clippy::too_many_arguments)]
pub fn polar_integral(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
) -> Result<PolarIntegralResult, EngineError> {
    validate_polar_request(expression, x, y, radius, angle, region)?;
    evaluate_polar(engine, expression, x, y, radius, angle, region, true)
}

#[allow(clippy::too_many_arguments)]
pub fn polar_integral_steps(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
) -> Result<PolarIntegralStepResult, EngineError> {
    polar_integral_steps_with_verbosity(
        engine,
        expression,
        x,
        y,
        radius,
        angle,
        region,
        StepVerbosity::Detailed,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn polar_integral_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
    verbosity: StepVerbosity,
) -> Result<PolarIntegralStepResult, EngineError> {
    validate_polar_request(expression, x, y, radius, angle, region)?;
    let mut result = evaluate_polar(engine, expression, x, y, radius, angle, region, false)?;
    let mut events = vec![
        StepEvent::new(
            "polar-coordinate-substitution",
            &format!(
                "{{{}=={},{}=={}}}",
                x, result.x_substitution, y, result.y_substitution
            ),
            "使用极坐标替换笛卡尔坐标。",
            StepImportance::Key,
        ),
        StepEvent::new(
            "polar-jacobian",
            &result.jacobian,
            "计算坐标变换的 Jacobian 行列式；在非负径向范围内面积因子为 r。",
            StepImportance::Normal,
        ),
        StepEvent::new(
            "polar-transform-integrand",
            &result.transformed_integrand,
            "替换被积式并乘以 Jacobian 面积因子。",
            StepImportance::Normal,
        ),
        StepEvent::new(
            "iterated-integral-inner",
            &result.integral.inner.value,
            if result.integral.inner.completed {
                "先计算径向定积分。"
            } else {
                "径向定积分未得到解析结果。"
            },
            StepImportance::Normal,
        ),
    ];
    if let Some(outer) = &result.integral.outer {
        events.push(StepEvent::new(
            "iterated-integral-outer",
            &outer.value,
            if outer.completed {
                "再计算角向定积分。"
            } else {
                "角向定积分未得到解析结果。"
            },
            StepImportance::Normal,
        ));
    }
    events.push(StepEvent::new(
        "polar-integral-result",
        &result.integral.value,
        match result.integral.status {
            IteratedIntegralStatus::Evaluated => "得到极坐标二重积分结果。",
            IteratedIntegralStatus::InnerUnresolved => "径向积分未解析完成。",
            IteratedIntegralStatus::OuterUnresolved => "角向积分未解析完成。",
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.integral.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(PolarIntegralStepResult { result, steps })
}

pub fn double_integral(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<DoubleIntegralResult, EngineError> {
    validate_request(expression, inner, outer)?;
    evaluate(engine, expression, inner, outer, true)
}

pub fn double_integral_steps(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<DoubleIntegralStepResult, EngineError> {
    double_integral_steps_with_verbosity(engine, expression, inner, outer, StepVerbosity::Detailed)
}

pub fn double_integral_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    inner_bound: IntegralBound<'_>,
    outer_bound: IntegralBound<'_>,
    verbosity: StepVerbosity,
) -> Result<DoubleIntegralStepResult, EngineError> {
    validate_request(expression, inner_bound, outer_bound)?;
    let mut result = evaluate(engine, expression, inner_bound, outer_bound, false)?;
    let setup = nested_expression(expression, inner_bound, outer_bound);
    let mut events = vec![StepEvent::new(
        "iterated-integral-setup",
        &setup,
        "按给定次序建立二重迭代积分，先计算内层积分。",
        StepImportance::Routine,
    )];
    events.push(StepEvent::new(
        "iterated-integral-inner",
        &result.inner.value,
        if result.inner.completed {
            "计算内层定积分，结果作为外层变量的函数。"
        } else {
            "内层定积分未得到解析结果，停止后续解析积分。"
        },
        if result.inner.completed {
            StepImportance::Normal
        } else {
            StepImportance::Key
        },
    ));
    if let Some(outer) = &result.outer {
        events.push(StepEvent::new(
            "iterated-integral-outer",
            &outer.value,
            if outer.completed {
                "对内层结果计算外层定积分。"
            } else {
                "外层定积分未得到解析结果。"
            },
            StepImportance::Normal,
        ));
    }
    events.push(StepEvent::new(
        "iterated-integral-result",
        &result.value,
        match result.status {
            IteratedIntegralStatus::Evaluated => "得到二重积分的解析结果。",
            IteratedIntegralStatus::InnerUnresolved => "内层积分未解析完成。",
            IteratedIntegralStatus::OuterUnresolved => "外层积分未解析完成。",
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(DoubleIntegralStepResult { result, steps })
}

pub fn triple_integral(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<TripleIntegralResult, EngineError> {
    validate_triple_request(expression, inner, middle, outer)?;
    evaluate_triple(engine, expression, inner, middle, outer, true)
}

pub fn triple_integral_steps(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<TripleIntegralStepResult, EngineError> {
    triple_integral_steps_with_verbosity(
        engine,
        expression,
        inner,
        middle,
        outer,
        StepVerbosity::Detailed,
    )
}

pub fn triple_integral_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
    verbosity: StepVerbosity,
) -> Result<TripleIntegralStepResult, EngineError> {
    validate_triple_request(expression, inner, middle, outer)?;
    let mut result = evaluate_triple(engine, expression, inner, middle, outer, false)?;
    let setup = nested_triple_expression(expression, inner, middle, outer);
    let mut events = vec![StepEvent::new(
        "triple-integral-setup",
        &setup,
        "按给定次序建立三重迭代积分。",
        StepImportance::Routine,
    )];
    let rules = [
        "triple-integral-inner",
        "triple-integral-middle",
        "triple-integral-outer",
    ];
    let explanations = [
        "先计算内层定积分，结果作为两个外层变量的函数。",
        "对内层结果计算中层定积分。",
        "对中层结果计算最外层定积分。",
    ];
    for (index, layer) in result.layers.iter().enumerate() {
        events.push(StepEvent::new(
            rules[index],
            &layer.value,
            if layer.completed {
                explanations[index]
            } else {
                "该层积分未得到解析结果，停止后续解析积分。"
            },
            if layer.completed {
                StepImportance::Normal
            } else {
                StepImportance::Key
            },
        ));
    }
    events.push(StepEvent::new(
        "triple-integral-result",
        &result.value,
        match result.status {
            TripleIntegralStatus::Evaluated => "得到三重积分的解析结果。",
            TripleIntegralStatus::InnerUnresolved => "内层积分未解析完成。",
            TripleIntegralStatus::MiddleUnresolved => "中层积分未解析完成。",
            TripleIntegralStatus::OuterUnresolved => "外层积分未解析完成。",
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(TripleIntegralStepResult { result, steps })
}

fn validate_polar_request(
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
) -> Result<(), EngineError> {
    validate_expression(expression, "极坐标积分被积表达式")?;
    for (label, variable) in [
        ("笛卡尔横坐标", x),
        ("笛卡尔纵坐标", y),
        ("极径变量", radius),
        ("极角变量", angle),
    ] {
        validate_symbol(variable, label)?;
    }
    let mut variables = [x, y, radius, angle];
    variables.sort_unstable();
    if variables.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(EngineError::InvalidInput(
            "笛卡尔变量与极坐标变量必须彼此不同".into(),
        ));
    }
    let analysis = analyze_expression(expression, "极坐标积分被积表达式")?;
    if analysis
        .symbols
        .iter()
        .any(|symbol| symbol == radius || symbol == angle)
    {
        return Err(EngineError::InvalidInput(
            "原被积表达式不能包含极坐标目标变量".into(),
        ));
    }
    for (label, bound) in [
        ("极径下限", region.radial_lower),
        ("极径上限", region.radial_upper),
        ("极角下限", region.angle_lower),
        ("极角上限", region.angle_upper),
    ] {
        validate_expression(bound, label)?;
        let symbols = analyze_expression(bound, label)?.symbols;
        if symbols
            .iter()
            .any(|symbol| symbol == x || symbol == y || symbol == radius || symbol == angle)
        {
            return Err(EngineError::InvalidInput(format!(
                "{label}必须是与坐标变量无关的有限实数"
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_polar(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
    render_value: bool,
) -> Result<PolarIntegralResult, EngineError> {
    let x_substitution = format!("{radius}*Cos({angle})");
    let y_substitution = format!("{radius}*Sin({angle})");
    let [tx, ty, drx, dtx, dry, dty, jacobian_symbol, transformed, verified] =
        polar_temporary_symbols(expression, x, y, radius, angle, region);
    let transform = engine.eval_expr(&format!(
        "[Local({tx},{ty},{drx},{dtx},{dry},{dty},{jacobian_symbol},{transformed},{verified});{tx}:={x_substitution};{ty}:={y_substitution};{drx}:=Eval(ApplyPure(\"Deriv\",{{{radius},{tx}}}));{dtx}:=Eval(ApplyPure(\"Deriv\",{{{angle},{tx}}}));{dry}:=Eval(ApplyPure(\"Deriv\",{{{radius},{ty}}}));{dty}:=Eval(ApplyPure(\"Deriv\",{{{angle},{ty}}}));{verified}:=IsZero(Simplify({drx}-Cos({angle}))) And IsZero(Simplify({dtx}+{radius}*Sin({angle}))) And IsZero(Simplify({dry}-Sin({angle}))) And IsZero(Simplify({dty}-{radius}*Cos({angle})));{jacobian_symbol}:={radius};{transformed}:=Eval(ApplyPure(\"Subst\",{{{x},{tx},{expression}}}));{transformed}:=Eval(ApplyPure(\"Subst\",{{{y},{ty},{transformed}}}));{{{jacobian_symbol},{verified},TrigSimpCombineNested({transformed}*{radius}),N({}),N({}),N({}),N({})}};]",
        region.radial_lower,
        region.radial_upper,
        region.angle_lower,
        region.angle_upper
    ))?;
    let Expr::Call { head, args } = transform else {
        return Err(EngineError::Parse("极坐标转换结果不是列表".into()));
    };
    if head != "List" || args.len() != 7 {
        return Err(EngineError::Parse("极坐标转换结果形态异常".into()));
    }
    let jacobian = args[0].to_string();
    let jacobian_verified = boolean(&args[1], "极坐标 Jacobian 证书")?;
    if !jacobian_verified {
        return Err(EngineError::Parse(format!(
            "极坐标 Jacobian 未通过符号证书: {jacobian}"
        )));
    }
    let transformed_integrand = args[2].to_string();
    let region_values = validate_numeric_region(&args[3..7])?;
    let radial = IntegralBound {
        variable: radius,
        lower: region.radial_lower,
        upper: region.radial_upper,
    };
    let angular = IntegralBound {
        variable: angle,
        lower: region.angle_lower,
        upper: region.angle_upper,
    };
    let integral = evaluate(
        engine,
        &transformed_integrand,
        radial,
        angular,
        render_value,
    )?;
    let zero_radius = region_values[0].abs() <= 1e-12;
    let full_angle = (region_values[3] - region_values[2] - std::f64::consts::TAU).abs() <= 1e-10;
    let region_kind = match (zero_radius, full_angle) {
        (true, true) => PolarRegionKind::Disk,
        (false, true) => PolarRegionKind::Annulus,
        (true, false) => PolarRegionKind::Sector,
        (false, false) => PolarRegionKind::AnnularSector,
    };
    Ok(PolarIntegralResult {
        cartesian_expression: expression.into(),
        cartesian_variables: [x.into(), y.into()],
        polar_variables: [radius.into(), angle.into()],
        region_kind,
        x_substitution,
        y_substitution,
        jacobian,
        jacobian_verified,
        transformed_integrand,
        integral,
    })
}

#[allow(clippy::too_many_arguments)]
fn polar_temporary_symbols(
    expression: &str,
    x: &str,
    y: &str,
    radius: &str,
    angle: &str,
    region: PolarRegion<'_>,
) -> [String; 9] {
    fresh_internal_symbols(
        "Polar",
        &[
            expression,
            x,
            y,
            radius,
            angle,
            region.radial_lower,
            region.radial_upper,
            region.angle_lower,
            region.angle_upper,
        ],
        ["Tx", "Ty", "Drx", "Dtx", "Dry", "Dty", "J", "T", "V"],
    )
}

fn numeric_value(value: &Expr) -> Result<f64, EngineError> {
    let Expr::Number(value) = value else {
        return Err(EngineError::InvalidInput(format!(
            "极坐标区域边界不是有限实数: {value}"
        )));
    };
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| EngineError::InvalidInput("极坐标区域边界不是有限实数".into()))
}

fn validate_numeric_region(values: &[Expr]) -> Result<[f64; 4], EngineError> {
    let values = values
        .iter()
        .map(numeric_value)
        .collect::<Result<Vec<_>, _>>()?;
    if values[0] < 0.0 || values[1] <= values[0] {
        return Err(EngineError::InvalidInput(
            "极径范围必须满足 0 <= 下限 < 上限".into(),
        ));
    }
    if values[3] <= values[2] || values[3] - values[2] > std::f64::consts::TAU + 1e-10 {
        return Err(EngineError::InvalidInput(
            "极角范围必须递增且跨度不超过 2*Pi".into(),
        ));
    }
    Ok([values[0], values[1], values[2], values[3]])
}

fn validate_request(
    expression: &str,
    inner: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<(), EngineError> {
    validate_expression(expression, "二重积分被积表达式")?;
    for (label, bound) in [("内层", inner), ("外层", outer)] {
        validate_symbol(bound.variable, &format!("{label}积分变量"))?;
        validate_expression(bound.lower, &format!("{label}积分下限"))?;
        validate_expression(bound.upper, &format!("{label}积分上限"))?;
    }
    if inner.variable == outer.variable {
        return Err(EngineError::InvalidInput(
            "二重积分的两个变量必须不同".into(),
        ));
    }
    let outer_symbols = analyze_expression(
        &format!("{{{},{}}}", outer.lower, outer.upper),
        "外层积分上下限",
    )?
    .symbols;
    if outer_symbols
        .iter()
        .any(|symbol| symbol == inner.variable || symbol == outer.variable)
    {
        return Err(EngineError::InvalidInput(
            "外层积分上下限不能依赖积分变量".into(),
        ));
    }
    let inner_symbols = analyze_expression(
        &format!("{{{},{}}}", inner.lower, inner.upper),
        "内层积分上下限",
    )?
    .symbols;
    if inner_symbols.iter().any(|symbol| symbol == inner.variable) {
        return Err(EngineError::InvalidInput(
            "内层积分上下限不能依赖内层积分变量".into(),
        ));
    }
    Ok(())
}

fn evaluate(
    engine: &mut dyn Engine,
    expression: &str,
    inner_bound: IntegralBound<'_>,
    outer_bound: IntegralBound<'_>,
    render_value: bool,
) -> Result<DoubleIntegralResult, EngineError> {
    let [inner_value_symbol, outer_value_symbol, inner_complete_symbol, outer_complete_symbol] =
        fresh_internal_symbols(
            "DoubleIntegral",
            &[
                expression,
                inner_bound.variable,
                inner_bound.lower,
                inner_bound.upper,
                outer_bound.variable,
                outer_bound.lower,
                outer_bound.upper,
            ],
            ["Inner", "Outer", "InnerComplete", "OuterComplete"],
        );
    let inner_call = format!(
        "Integrate({},{},{})({expression})",
        inner_bound.variable, inner_bound.lower, inner_bound.upper
    );
    let data = engine.eval_expr(&format!(
        "[Local({inner_value_symbol},{outer_value_symbol},{inner_complete_symbol},{outer_complete_symbol}); {inner_value_symbol}:={inner_call}; {inner_complete_symbol}:=IsFreeOf(Integrate,{inner_value_symbol}); If({inner_complete_symbol},[{outer_value_symbol}:=Integrate({},{},{}){inner_value_symbol};{outer_complete_symbol}:=IsFreeOf(Integrate,{outer_value_symbol});], [{outer_value_symbol}:=Undefined;{outer_complete_symbol}:=False;]); {{{inner_value_symbol},{inner_complete_symbol},{outer_value_symbol},{outer_complete_symbol}}};]",
        outer_bound.variable, outer_bound.lower, outer_bound.upper
    ))?;
    let Expr::Call { head, args } = data else {
        return Err(EngineError::Parse("二重积分结果不是列表".into()));
    };
    if head != "List" || args.len() != 4 {
        return Err(EngineError::Parse("二重积分结果形态异常".into()));
    }
    let inner_value = args[0].to_string();
    let inner_completed = boolean(&args[1], "内层积分状态")?;
    let outer_value = args[2].to_string();
    let outer_completed = boolean(&args[3], "外层积分状态")?;
    let status = if !inner_completed {
        IteratedIntegralStatus::InnerUnresolved
    } else if !outer_completed {
        IteratedIntegralStatus::OuterUnresolved
    } else {
        IteratedIntegralStatus::Evaluated
    };
    let value = if inner_completed {
        outer_value.clone()
    } else {
        inner_value.clone()
    };
    let tex = if render_value {
        render_one_tex(engine, &value)?
    } else {
        String::new()
    };
    Ok(DoubleIntegralResult {
        status,
        expression: expression.into(),
        inner: IntegralLayerResult {
            variable: inner_bound.variable.into(),
            lower: inner_bound.lower.into(),
            upper: inner_bound.upper.into(),
            integrand: expression.into(),
            value: inner_value,
            completed: inner_completed,
        },
        outer: inner_completed.then(|| IntegralLayerResult {
            variable: outer_bound.variable.into(),
            lower: outer_bound.lower.into(),
            upper: outer_bound.upper.into(),
            integrand: args[0].to_string(),
            value: outer_value,
            completed: outer_completed,
        }),
        value,
        tex,
    })
}

fn validate_triple_request(
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> Result<(), EngineError> {
    validate_expression(expression, "三重积分被积表达式")?;
    let bounds = [inner, middle, outer];
    for (label, bound) in [("内层", inner), ("中层", middle), ("外层", outer)] {
        validate_symbol(bound.variable, &format!("{label}积分变量"))?;
        validate_expression(bound.lower, &format!("{label}积分下限"))?;
        validate_expression(bound.upper, &format!("{label}积分上限"))?;
    }
    let mut variables = bounds.map(|bound| bound.variable);
    variables.sort_unstable();
    if variables.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(EngineError::InvalidInput(
            "三重积分的三个变量必须不同".into(),
        ));
    }
    for (index, bound) in bounds.iter().enumerate() {
        let symbols = analyze_expression(
            &format!("{{{},{}}}", bound.lower, bound.upper),
            "三重积分上下限",
        )?
        .symbols;
        if bounds[..=index]
            .iter()
            .any(|forbidden| symbols.iter().any(|symbol| symbol == forbidden.variable))
        {
            return Err(EngineError::InvalidInput(
                match index {
                    0 => "内层积分上下限不能依赖内层积分变量",
                    1 => "中层积分上下限只能依赖最外层积分变量",
                    _ => "外层积分上下限不能依赖积分变量",
                }
                .into(),
            ));
        }
    }
    Ok(())
}

fn evaluate_triple(
    engine: &mut dyn Engine,
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
    render_value: bool,
) -> Result<TripleIntegralResult, EngineError> {
    let [inner_value_symbol, middle_value_symbol, outer_value_symbol, inner_complete_symbol, middle_complete_symbol, outer_complete_symbol] =
        fresh_internal_symbols(
            "TripleIntegral",
            &[
                expression,
                inner.variable,
                inner.lower,
                inner.upper,
                middle.variable,
                middle.lower,
                middle.upper,
                outer.variable,
                outer.lower,
                outer.upper,
            ],
            [
                "Inner",
                "Middle",
                "Outer",
                "InnerComplete",
                "MiddleComplete",
                "OuterComplete",
            ],
        );
    let inner_call = format!(
        "Integrate({},{},{})({expression})",
        inner.variable, inner.lower, inner.upper
    );
    let data = engine.eval_expr(&format!(
        "[Local({inner_value_symbol},{middle_value_symbol},{outer_value_symbol},{inner_complete_symbol},{middle_complete_symbol},{outer_complete_symbol}); {inner_value_symbol}:={inner_call}; {inner_complete_symbol}:=IsFreeOf(Integrate,{inner_value_symbol}); \
         If({inner_complete_symbol},[{middle_value_symbol}:=Integrate({},{},{}){inner_value_symbol};{middle_complete_symbol}:=IsFreeOf(Integrate,{middle_value_symbol});],[{middle_value_symbol}:=Undefined;{middle_complete_symbol}:=False;]); \
         If({middle_complete_symbol},[{outer_value_symbol}:=Integrate({},{},{}){middle_value_symbol};{outer_complete_symbol}:=IsFreeOf(Integrate,{outer_value_symbol});],[{outer_value_symbol}:=Undefined;{outer_complete_symbol}:=False;]); \
         {{{inner_value_symbol},{inner_complete_symbol},{middle_value_symbol},{middle_complete_symbol},{outer_value_symbol},{outer_complete_symbol}}};]",
        middle.variable, middle.lower, middle.upper, outer.variable, outer.lower, outer.upper,
    ))?;
    let Expr::Call { head, args } = data else {
        return Err(EngineError::Parse("三重积分结果不是列表".into()));
    };
    if head != "List" || args.len() != 6 {
        return Err(EngineError::Parse("三重积分结果形态异常".into()));
    }
    let completed = [
        boolean(&args[1], "内层积分状态")?,
        boolean(&args[3], "中层积分状态")?,
        boolean(&args[5], "外层积分状态")?,
    ];
    let status = match completed {
        [false, _, _] => TripleIntegralStatus::InnerUnresolved,
        [true, false, _] => TripleIntegralStatus::MiddleUnresolved,
        [true, true, false] => TripleIntegralStatus::OuterUnresolved,
        [true, true, true] => TripleIntegralStatus::Evaluated,
    };
    let values = [&args[0], &args[2], &args[4]];
    let bounds = [inner, middle, outer];
    let reached = if !completed[0] {
        1
    } else if !completed[1] {
        2
    } else {
        3
    };
    let mut layers = Vec::with_capacity(reached);
    for index in 0..reached {
        layers.push(IntegralLayerResult {
            variable: bounds[index].variable.into(),
            lower: bounds[index].lower.into(),
            upper: bounds[index].upper.into(),
            integrand: if index == 0 {
                expression.into()
            } else {
                values[index - 1].to_string()
            },
            value: values[index].to_string(),
            completed: completed[index],
        });
    }
    let value = layers
        .last()
        .expect("a triple integral always reaches its inner layer")
        .value
        .clone();
    let tex = if render_value {
        render_one_tex(engine, &value)?
    } else {
        String::new()
    };
    Ok(TripleIntegralResult {
        status,
        expression: expression.into(),
        layers,
        value,
        tex,
    })
}

fn nested_expression(
    expression: &str,
    inner: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> String {
    format!(
        "Integrate({},{},{})Integrate({},{},{})({expression})",
        outer.variable, outer.lower, outer.upper, inner.variable, inner.lower, inner.upper
    )
}

fn nested_triple_expression(
    expression: &str,
    inner: IntegralBound<'_>,
    middle: IntegralBound<'_>,
    outer: IntegralBound<'_>,
) -> String {
    format!(
        "Integrate({},{},{})Integrate({},{},{})Integrate({},{},{})({expression})",
        outer.variable,
        outer.lower,
        outer.upper,
        middle.variable,
        middle.lower,
        middle.upper,
        inner.variable,
        inner.lower,
        inner.upper,
    )
}

fn boolean(value: &Expr, label: &str) -> Result<bool, EngineError> {
    match value {
        Expr::Symbol(value) if value == "True" => Ok(true),
        Expr::Symbol(value) if value == "False" => Ok(false),
        other => Err(EngineError::Parse(format!("{label}不是布尔值: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::test_support::CountingEngine;

    #[test]
    fn evaluates_rectangular_and_variable_bound_regions() {
        let mut engine = RustEngine::spawn().unwrap();
        let rectangle = double_integral(
            &mut engine,
            "x+y",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "2",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(rectangle.status, IteratedIntegralStatus::Evaluated);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-3)", rectangle.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );

        let triangle = double_integral(
            &mut engine,
            "x*y",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "x",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(triangle.status, IteratedIntegralStatus::Evaluated);
        assert_eq!(
            engine
                .eval(&format!("Simplify(({})-1/8)", triangle.value))
                .unwrap()
                .expr
                .to_string(),
            "0"
        );
    }

    #[test]
    fn returns_layered_steps_and_honest_unresolved_status() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = double_integral_steps(
            &mut engine,
            "x+y",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "x",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(result.result.status, IteratedIntegralStatus::Evaluated);
        assert!(result
            .steps
            .iter()
            .any(|step| step.rule == "iterated-integral-inner"));
        assert_eq!(
            result.steps.last().unwrap().rule,
            "iterated-integral-result"
        );
        assert!(!result.result.tex.is_empty());

        let unresolved = double_integral(
            &mut engine,
            "Sin(y^y)",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "x",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(unresolved.status, IteratedIntegralStatus::InnerUnresolved);
        assert!(unresolved.outer.is_none());

        let outer_unresolved = double_integral(
            &mut engine,
            "Sin(x^x)",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(
            outer_unresolved.status,
            IteratedIntegralStatus::OuterUnresolved
        );
        assert!(outer_unresolved.outer.is_some());
    }

    #[test]
    fn validates_order_and_step_verbosity() {
        let mut engine = RustEngine::spawn().unwrap();
        let inner = IntegralBound {
            variable: "y",
            lower: "0",
            upper: "x",
        };
        let outer = IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        };
        let concise = double_integral_steps_with_verbosity(
            &mut engine,
            "x+y",
            inner,
            outer,
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(concise.steps.len(), 1);
        assert_eq!(concise.steps[0].rule, "iterated-integral-result");

        assert!(double_integral(
            &mut engine,
            "x+y",
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1"
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1"
            },
        )
        .is_err());
        assert!(double_integral(
            &mut engine,
            "x+y",
            inner,
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "y"
            },
        )
        .is_err());
    }

    #[test]
    fn evaluates_verified_polar_disks_annuli_and_sectors() {
        let mut engine = RustEngine::spawn().unwrap();
        let disk = polar_integral(
            &mut engine,
            "1",
            "x",
            "y",
            "r",
            "theta",
            PolarRegion {
                radial_lower: "0",
                radial_upper: "2",
                angle_lower: "0",
                angle_upper: "2*Pi",
            },
        )
        .unwrap();
        assert_eq!(disk.region_kind, PolarRegionKind::Disk);
        assert!(disk.jacobian_verified);
        assert_eq!(disk.jacobian, "r");
        assert_eq!(
            engine
                .eval(&format!("IsZero(Simplify(({})-4*Pi))", disk.integral.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        let template = polar_integral(
            &mut engine,
            "x^2+y^2",
            "x",
            "y",
            "r",
            "t",
            PolarRegion {
                radial_lower: "0",
                radial_upper: "1",
                angle_lower: "0",
                angle_upper: "2*Pi",
            },
        )
        .unwrap();
        assert_eq!(template.region_kind, PolarRegionKind::Disk);
        assert_eq!(template.integral.status, IteratedIntegralStatus::Evaluated);
        assert_eq!(
            engine
                .eval(&format!(
                    "IsZero(Simplify(({})-Pi/2))",
                    template.integral.value
                ))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        let annulus = polar_integral(
            &mut engine,
            "1",
            "x",
            "y",
            "rho",
            "phi",
            PolarRegion {
                radial_lower: "1",
                radial_upper: "2",
                angle_lower: "0",
                angle_upper: "2*Pi",
            },
        )
        .unwrap();
        assert_eq!(annulus.region_kind, PolarRegionKind::Annulus);
        assert_eq!(
            engine
                .eval(&format!(
                    "IsZero(Simplify(({})-3*Pi))",
                    annulus.integral.value
                ))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        let sector = polar_integral(
            &mut engine,
            "x^2+y^2",
            "x",
            "y",
            "r",
            "theta",
            PolarRegion {
                radial_lower: "0",
                radial_upper: "2",
                angle_lower: "0",
                angle_upper: "Pi/2",
            },
        )
        .unwrap();
        assert_eq!(sector.region_kind, PolarRegionKind::Sector);
        assert_eq!(
            engine
                .eval(&format!(
                    "IsZero(Simplify(({})-2*Pi))",
                    sector.integral.value
                ))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );
    }

    #[test]
    fn evaluates_gaussian_disk_after_nested_trigonometric_normalization() {
        let mut engine = RustEngine::spawn().unwrap();
        let gaussian = polar_integral_steps(
            &mut engine,
            "Exp(-(x^2+y^2))",
            "x",
            "y",
            "r",
            "theta",
            PolarRegion {
                radial_lower: "0",
                radial_upper: "2",
                angle_lower: "0",
                angle_upper: "2*Pi",
            },
        )
        .unwrap();
        assert_eq!(
            gaussian.result.integral.status,
            IteratedIntegralStatus::Evaluated,
            "{gaussian:#?}"
        );
        assert!(!gaussian.result.transformed_integrand.contains("theta"));
        assert!(gaussian
            .steps
            .iter()
            .any(|step| step.rule == "iterated-integral-outer"));
        assert_eq!(
            gaussian.steps.last().map(|step| step.rule.as_str()),
            Some("polar-integral-result")
        );
        let numeric = engine
            .eval(&format!("N({})", gaussian.result.integral.value))
            .unwrap();
        let actual = numeric_value(&numeric.expr).unwrap();
        let expected = std::f64::consts::PI * (1.0 - (-4.0_f64).exp());
        assert!((actual - expected).abs() < 1e-9, "{gaussian:#?}");
    }

    #[test]
    fn polar_steps_expose_transform_and_reject_invalid_regions() {
        let mut engine = RustEngine::spawn().unwrap();
        let region = PolarRegion {
            radial_lower: "1",
            radial_upper: "2",
            angle_lower: "0",
            angle_upper: "Pi/2",
        };
        let stepped =
            polar_integral_steps(&mut engine, "x", "x", "y", "r", "theta", region).unwrap();
        assert_eq!(stepped.result.region_kind, PolarRegionKind::AnnularSector);
        for rule in [
            "polar-coordinate-substitution",
            "polar-jacobian",
            "polar-transform-integrand",
            "polar-integral-result",
        ] {
            assert!(stepped.steps.iter().any(|step| step.rule == rule));
        }

        assert!(polar_integral(
            &mut engine,
            "1",
            "x",
            "y",
            "r",
            "theta",
            PolarRegion {
                radial_lower: "-1",
                radial_upper: "2",
                angle_lower: "0",
                angle_upper: "2*Pi",
            },
        )
        .is_err());
        assert!(polar_integral(&mut engine, "r+x", "x", "y", "r", "theta", region,).is_err());
    }

    #[test]
    fn iterated_integral_steps_replace_result_tex_with_one_filtered_batch() {
        let inner = IntegralBound {
            variable: "y",
            lower: "0",
            upper: "1",
        };
        let outer = IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        };
        let mut engine = CountingEngine::spawn();
        double_integral(&mut engine, "x+y", inner, outer).unwrap();
        assert_eq!(engine.batch_sizes, [1]);

        engine.reset_counts();
        let detailed = double_integral_steps_with_verbosity(
            &mut engine,
            "x+y",
            inner,
            outer,
            StepVerbosity::Detailed,
        )
        .unwrap();
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);

        engine.reset_counts();
        let concise = double_integral_steps_with_verbosity(
            &mut engine,
            "x+y",
            inner,
            outer,
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
        assert!(concise.steps.len() < detailed.steps.len());

        let region = PolarRegion {
            radial_lower: "0",
            radial_upper: "1",
            angle_lower: "0",
            angle_upper: "Pi/2",
        };
        engine.reset_counts();
        polar_integral(&mut engine, "1", "x", "y", "r", "theta", region).unwrap();
        assert_eq!(engine.eval_calls, 2);
        assert_eq!(engine.batch_sizes, [1]);

        engine.reset_counts();
        let polar = polar_integral_steps_with_verbosity(
            &mut engine,
            "1",
            "x",
            "y",
            "r",
            "theta",
            region,
            StepVerbosity::Concise,
        )
        .unwrap();
        assert_eq!(engine.eval_calls, 2);
        assert_eq!(engine.batch_sizes, [polar.steps.len()]);
    }

    #[test]
    fn evaluates_rectangular_and_variable_bound_triple_integrals() {
        let mut engine = RustEngine::spawn().unwrap();
        let rectangular = triple_integral(
            &mut engine,
            "x+y+z",
            IntegralBound {
                variable: "z",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(rectangular.status, TripleIntegralStatus::Evaluated);
        assert_eq!(rectangular.layers.len(), 3);
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-3/2)", rectangular.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        let variable_bounds = triple_integral(
            &mut engine,
            "1",
            IntegralBound {
                variable: "z",
                lower: "0",
                upper: "x+y",
            },
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "x",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(variable_bounds.status, TripleIntegralStatus::Evaluated);
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-1/2)", variable_bounds.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );
    }

    #[test]
    fn iterated_integrals_preserve_symbols_matching_former_temporaries() {
        let mut engine = RustEngine::spawn().unwrap();
        let double = double_integral(
            &mut engine,
            "i",
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(double.status, IteratedIntegralStatus::Evaluated);
        assert_eq!(double.value, "i");

        let triple = triple_integral(
            &mut engine,
            "m",
            IntegralBound {
                variable: "z",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "1",
            },
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "1",
            },
        )
        .unwrap();
        assert_eq!(triple.status, TripleIntegralStatus::Evaluated);
        assert_eq!(triple.value, "m");
    }

    #[test]
    fn triple_integral_stops_at_unresolved_layer_and_filters_steps() {
        let inner = IntegralBound {
            variable: "z",
            lower: "0",
            upper: "1",
        };
        let middle = IntegralBound {
            variable: "y",
            lower: "0",
            upper: "1",
        };
        let outer = IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        };
        let mut engine = CountingEngine::spawn();
        let unresolved = triple_integral(&mut engine, "Sin(y^y)", inner, middle, outer).unwrap();
        assert_eq!(unresolved.status, TripleIntegralStatus::MiddleUnresolved);
        assert_eq!(unresolved.layers.len(), 2);

        engine.reset_counts();
        let detailed = triple_integral_steps(&mut engine, "x+y+z", inner, middle, outer).unwrap();
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);
        engine.reset_counts();
        let concise = triple_integral_steps_with_verbosity(
            &mut engine,
            "x+y+z",
            inner,
            middle,
            outer,
            StepVerbosity::Concise,
        )
        .unwrap();
        assert!(concise.steps.len() < detailed.steps.len());
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
    }

    #[test]
    fn triple_integral_rejects_invalid_bound_dependencies() {
        let mut engine = RustEngine::spawn().unwrap();
        let valid_inner = IntegralBound {
            variable: "z",
            lower: "0",
            upper: "x+y",
        };
        let valid_middle = IntegralBound {
            variable: "y",
            lower: "0",
            upper: "x",
        };
        let valid_outer = IntegralBound {
            variable: "x",
            lower: "0",
            upper: "1",
        };
        assert!(triple_integral(
            &mut engine,
            "1",
            IntegralBound {
                variable: "z",
                lower: "0",
                upper: "z",
            },
            valid_middle,
            valid_outer,
        )
        .is_err());
        assert!(triple_integral(
            &mut engine,
            "1",
            valid_inner,
            IntegralBound {
                variable: "y",
                lower: "0",
                upper: "z",
            },
            valid_outer,
        )
        .is_err());
        assert!(triple_integral(
            &mut engine,
            "1",
            valid_inner,
            valid_middle,
            IntegralBound {
                variable: "x",
                lower: "0",
                upper: "y",
            },
        )
        .is_err());
    }
}
