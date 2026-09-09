//! Structured scalar and vector line integrals on bounded parametric curves.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{analyze_expression, strip_tex_delimiters, validate_symbol};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineIntegralKind {
    ScalarArcLength,
    VectorWork,
}

#[derive(Debug, Clone)]
pub struct LineIntegralRequest {
    pub kind: LineIntegralKind,
    pub field: Vec<String>,
    pub coordinates: Vec<String>,
    pub curve: Vec<String>,
    pub parameter: String,
    pub lower: String,
    pub upper: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineIntegralResult {
    pub kind: LineIntegralKind,
    pub dimension: usize,
    pub curve: Vec<String>,
    pub velocity: Vec<String>,
    pub speed: Option<String>,
    pub field_on_curve: Vec<String>,
    pub integrand: String,
    pub value: String,
    pub completed: bool,
    pub integrand_verified: bool,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineIntegralStepResult {
    pub result: LineIntegralResult,
    pub steps: Vec<Step>,
}

pub fn compute(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
) -> Result<LineIntegralResult, EngineError> {
    validate_request(request)?;
    evaluate(engine, request, true)
}

pub fn compute_steps(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
) -> Result<LineIntegralStepResult, EngineError> {
    compute_steps_with_verbosity(engine, request, StepVerbosity::Detailed)
}

pub fn compute_steps_with_verbosity(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
    verbosity: StepVerbosity,
) -> Result<LineIntegralStepResult, EngineError> {
    validate_request(request)?;
    let mut result = evaluate(engine, request, false)?;
    let mut events = vec![StepEvent::new(
        "line-integral-parameterize",
        &format!("{{{}}}", result.curve.join(",")),
        "使用给定参数表示积分曲线。",
        StepImportance::Key,
    )];
    events.push(StepEvent::new(
        "line-integral-tangent",
        &format!("{{{}}}", result.velocity.join(",")),
        "对参数曲线逐分量求导，得到切向量。",
        StepImportance::Normal,
    ));
    if request.kind == LineIntegralKind::ScalarArcLength {
        events.push(StepEvent::new(
            "line-integral-speed",
            result
                .speed
                .as_deref()
                .expect("scalar arc-length results include speed"),
            "计算切向量的长度，得到弧长微元的系数。",
            StepImportance::Normal,
        ));
    }
    events.push(StepEvent::new(
        "line-integral-pullback",
        &format!("{{{}}}", result.field_on_curve.join(",")),
        "将参数曲线代入被积函数或向量场。",
        StepImportance::Normal,
    ));
    events.push(StepEvent::new(
        "line-integral-integrand",
        &format!(
            "Integrate({},{},{})({})",
            request.parameter, request.lower, request.upper, result.integrand
        ),
        match request.kind {
            LineIntegralKind::ScalarArcLength => "乘以曲线速度，化为参数上的定积分。",
            LineIntegralKind::VectorWork => "与切向量作点积，化为参数上的定积分。",
        },
        StepImportance::Key,
    ));
    events.push(StepEvent::new(
        "line-integral-result",
        &result.value,
        if result.completed {
            "计算参数区间上的定积分。"
        } else {
            "当前积分规则未能得到解析结果。"
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(LineIntegralStepResult { result, steps })
}

fn evaluate(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
    render_value: bool,
) -> Result<LineIntegralResult, EngineError> {
    let velocity = request
        .curve
        .iter()
        .map(|component| format!("Deriv({})({component})", request.parameter))
        .collect::<Vec<_>>();
    let field_on_curve = request
        .field
        .iter()
        .map(|component| substitute_curve(component, &request.coordinates, &request.curve))
        .collect::<Vec<_>>();
    let speed_squared = velocity
        .iter()
        .map(|component| format!("({component})^2"))
        .collect::<Vec<_>>()
        .join("+");
    let reduce_trigonometric_speed = request.kind == LineIntegralKind::ScalarArcLength
        && request.curve.iter().any(|component| {
            analyze_expression(component, "参数曲线分量").is_ok_and(|analysis| {
                analysis
                    .function_heads
                    .iter()
                    .any(|head| head == "Sin" || head == "Cos")
            })
        });
    let speed_input = if reduce_trigonometric_speed {
        format!("TrigSimpCombine({speed_squared})")
    } else {
        speed_squared.clone()
    };
    let speed_command = if request.kind == LineIntegralKind::ScalarArcLength {
        format!("Simplify(Sqrt({speed_input}))")
    } else {
        "Undefined".into()
    };
    let raw_integrand = match request.kind {
        LineIntegralKind::ScalarArcLength => "f[1]*s".into(),
        LineIntegralKind::VectorWork => (1..=request.coordinates.len())
            .map(|index| format!("f[{index}]*v[{index}]"))
            .collect::<Vec<_>>()
            .join("+"),
    };
    let command = format!(
        "[Local(v,s,f,g,i,c); v:=Simplify({velocity}); s:={speed_command}; \
         f:=Simplify({field}); g:=Simplify({raw_integrand}); \
         i:=Integrate({parameter},{lower},{upper})g; c:=IsZero(Simplify(g-({raw_integrand}))); \
         {{v,s,f,g,i,IsFreeOf(Integrate,i),c}};]",
        velocity = list(&velocity),
        field = list(&field_on_curve),
        parameter = request.parameter,
        lower = request.lower,
        upper = request.upper,
    );
    let data = engine.eval_expr(&command)?;
    let fields = list_expr(&data, "线积分结果")?;
    if fields.len() != 7 {
        return Err(EngineError::Parse("线积分结果字段数量异常".into()));
    }
    let velocity = strings(list_expr(&fields[0], "切向量")?);
    let speed = (request.kind == LineIntegralKind::ScalarArcLength).then(|| fields[1].to_string());
    let field_on_curve = strings(list_expr(&fields[2], "曲线上的场")?);
    let integrand = fields[3].to_string();
    let value = fields[4].to_string();
    let completed = boolean(&fields[5], "线积分完成状态")?;
    let integrand_verified = boolean(&fields[6], "线积分被积式证书")?;
    if !integrand_verified {
        return Err(EngineError::Eval("线积分被积式证书失败".into()));
    }
    let tex = if render_value {
        strip_tex_delimiters(&engine.render_tex_batch(std::slice::from_ref(&value))?[0])
    } else {
        String::new()
    };
    Ok(LineIntegralResult {
        kind: request.kind,
        dimension: request.coordinates.len(),
        curve: request.curve.clone(),
        velocity,
        speed,
        field_on_curve,
        integrand,
        value,
        completed,
        integrand_verified,
        tex,
    })
}

fn validate_request(request: &LineIntegralRequest) -> Result<(), EngineError> {
    let dimension = request.coordinates.len();
    if !(2..=3).contains(&dimension) || request.curve.len() != dimension {
        return Err(EngineError::InvalidInput(
            "线积分只接受维度一致的二维或三维参数曲线".into(),
        ));
    }
    validate_symbol(&request.parameter, "曲线参数")?;
    let mut unique = BTreeSet::new();
    for coordinate in &request.coordinates {
        validate_symbol(coordinate, "坐标变量")?;
        if coordinate == &request.parameter || !unique.insert(coordinate) {
            return Err(EngineError::InvalidInput(
                "坐标变量必须互异且不能与曲线参数相同".into(),
            ));
        }
    }
    let expected_fields = match request.kind {
        LineIntegralKind::ScalarArcLength => 1,
        LineIntegralKind::VectorWork => dimension,
    };
    if request.field.len() != expected_fields {
        return Err(EngineError::InvalidInput(
            "被积函数或向量场的分量数量与线积分类型不匹配".into(),
        ));
    }
    for field in &request.field {
        let analysis = analyze_expression(field, "线积分场分量")?;
        if analysis.symbols.contains(&request.parameter) {
            return Err(EngineError::InvalidInput(
                "线积分场分量不能直接依赖曲线参数".into(),
            ));
        }
    }
    for component in &request.curve {
        let analysis = analyze_expression(component, "参数曲线分量")?;
        if analysis
            .symbols
            .iter()
            .any(|symbol| request.coordinates.contains(symbol))
        {
            return Err(EngineError::InvalidInput(
                "参数曲线分量不能依赖被替换的坐标变量".into(),
            ));
        }
    }
    for (bound, label) in [(&request.lower, "参数下限"), (&request.upper, "参数上限")] {
        let analysis = analyze_expression(bound, label)?;
        if analysis.symbols.contains(&request.parameter) {
            return Err(EngineError::InvalidInput(
                "参数区间端点不能依赖曲线参数".into(),
            ));
        }
    }
    Ok(())
}

fn substitute_curve(expression: &str, coordinates: &[String], curve: &[String]) -> String {
    coordinates.iter().zip(curve).fold(
        expression.to_string(),
        |current, (coordinate, component)| format!("Subst({coordinate},{component})({current})"),
    )
}

fn list(values: &[String]) -> String {
    format!("{{{}}}", values.join(","))
}

fn list_expr<'a>(expression: &'a Expr, label: &str) -> Result<&'a [Expr], EngineError> {
    match expression {
        Expr::Call { head, args } if head == "List" => Ok(args),
        other => Err(EngineError::Parse(format!("{label}不是列表: {other}"))),
    }
}

fn strings(expressions: &[Expr]) -> Vec<String> {
    expressions.iter().map(ToString::to_string).collect()
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
    use crate::engine::{Engine, RustEngine};
    use crate::test_support::CountingEngine;

    fn request(kind: LineIntegralKind, field: &[&str], curve: &[&str]) -> LineIntegralRequest {
        LineIntegralRequest {
            kind,
            field: field.iter().map(|value| (*value).into()).collect(),
            coordinates: vec!["x".into(), "y".into()],
            curve: curve.iter().map(|value| (*value).into()).collect(),
            parameter: "t".into(),
            lower: "0".into(),
            upper: "1".into(),
        }
    }

    #[test]
    fn computes_scalar_arc_length_and_vector_work_integrals() {
        let mut engine = RustEngine::spawn().unwrap();
        let scalar = compute(
            &mut engine,
            &request(LineIntegralKind::ScalarArcLength, &["x"], &["t", "0"]),
        )
        .unwrap();
        assert!(scalar.completed && scalar.integrand_verified);
        assert_eq!(scalar.speed.as_deref(), Some("1"));
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-1/2)", scalar.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        let work = compute(
            &mut engine,
            &request(LineIntegralKind::VectorWork, &["y", "x"], &["t", "t^2"]),
        )
        .unwrap();
        assert!(work.completed && work.integrand_verified);
        assert_eq!(work.value, "1");

        let mut circle = request(
            LineIntegralKind::ScalarArcLength,
            &["1"],
            &["Cos(t)", "Sin(t)"],
        );
        circle.upper = "2*Pi".into();
        let circumference = compute(&mut engine, &circle).unwrap();
        assert!(circumference.completed);
        assert_eq!(circumference.speed.as_deref(), Some("1"));
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-2*Pi)", circumference.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );
    }

    #[test]
    fn supports_three_dimensions_and_filtered_steps() {
        let mut request = request(
            LineIntegralKind::VectorWork,
            &["x", "y", "z"],
            &["t", "t^2", "t^3"],
        );
        request.coordinates.push("z".into());
        let mut engine = CountingEngine::spawn();
        let detailed = compute_steps(&mut engine, &request).unwrap();
        assert_eq!(detailed.result.dimension, 3);
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-3/2)", detailed.result.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );

        engine.reset_counts();
        let concise =
            compute_steps_with_verbosity(&mut engine, &request, StepVerbosity::Concise).unwrap();
        assert!(concise.steps.len() < detailed.steps.len());
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
    }

    #[test]
    fn rejects_dimension_mismatches_dependencies_and_injection() {
        let mut engine = RustEngine::spawn().unwrap();
        let mut invalid = request(LineIntegralKind::VectorWork, &["x"], &["t", "0"]);
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(LineIntegralKind::ScalarArcLength, &["1"], &["x+t", "0"]);
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(LineIntegralKind::ScalarArcLength, &["t"], &["t", "0"]);
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(
            LineIntegralKind::ScalarArcLength,
            &["1"],
            &["t);Echo(1);(t", "0"],
        );
        assert!(compute(&mut engine, &invalid).is_err());
    }
}
