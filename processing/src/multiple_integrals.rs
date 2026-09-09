//! Structured bounded iterated integrals.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, strip_tex_delimiters, validate_expression, validate_symbol,
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
    let transform = engine.eval_expr(&format!(
        "[Local(tx,ty,drx,dtx,dry,dty,j,t,v);tx:={x_substitution};ty:={y_substitution};drx:=Eval(ApplyPure(\"Deriv\",{{{radius},tx}}));dtx:=Eval(ApplyPure(\"Deriv\",{{{angle},tx}}));dry:=Eval(ApplyPure(\"Deriv\",{{{radius},ty}}));dty:=Eval(ApplyPure(\"Deriv\",{{{angle},ty}}));v:=IsZero(Simplify(drx-Cos({angle}))) And IsZero(Simplify(dtx+{radius}*Sin({angle}))) And IsZero(Simplify(dry-Sin({angle}))) And IsZero(Simplify(dty-{radius}*Cos({angle})));j:={radius};t:=Eval(ApplyPure(\"Subst\",{{{x},tx,{expression}}}));t:=Eval(ApplyPure(\"Subst\",{{{y},ty,t}}));{{j,v,Simplify(TrigSimpCombine(t*{radius})),N({}),N({}),N({}),N({})}};]",
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
    let inner_call = format!(
        "Integrate({},{},{})({expression})",
        inner_bound.variable, inner_bound.lower, inner_bound.upper
    );
    let data = engine.eval_expr(&format!(
        "[Local(i,o,ic,oc); i:={inner_call}; ic:=IsFreeOf(Integrate,i); If(ic,[o:=Integrate({},{},{})i;oc:=IsFreeOf(Integrate,o);], [o:=Undefined;oc:=False;]); {{i,ic,o,oc}};]",
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
        strip_tex_delimiters(&engine.render_tex_batch(std::slice::from_ref(&value))?[0])
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
        assert_eq!(engine.batch_sizes, [polar.steps.len()]);
    }
}
