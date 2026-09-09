//! Structured scalar-area and vector-flux integrals on parametric surfaces.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{analyze_expression, strip_tex_delimiters, validate_symbol};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceIntegralKind {
    ScalarArea,
    VectorFlux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceOrientation {
    ParameterOrder,
    Reversed,
}

#[derive(Debug, Clone)]
pub struct SurfaceIntegralRequest {
    pub kind: SurfaceIntegralKind,
    pub orientation: SurfaceOrientation,
    pub field: Vec<String>,
    pub coordinates: [String; 3],
    pub surface: [String; 3],
    pub parameters: [String; 2],
    pub lower: [String; 2],
    pub upper: [String; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceIntegralResult {
    pub kind: SurfaceIntegralKind,
    pub orientation: SurfaceOrientation,
    pub surface: [String; 3],
    pub tangent_u: Vec<String>,
    pub tangent_v: Vec<String>,
    pub oriented_normal: Vec<String>,
    pub area_factor: Option<String>,
    pub field_on_surface: Vec<String>,
    pub integrand: String,
    pub inner_value: String,
    pub value: String,
    pub inner_completed: bool,
    pub completed: bool,
    pub normal_verified: bool,
    pub integrand_verified: bool,
    pub tex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceIntegralStepResult {
    pub result: SurfaceIntegralResult,
    pub steps: Vec<Step>,
}

pub fn compute(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
) -> Result<SurfaceIntegralResult, EngineError> {
    validate_request(request)?;
    evaluate(engine, request, true)
}

pub fn compute_steps(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
) -> Result<SurfaceIntegralStepResult, EngineError> {
    compute_steps_with_verbosity(engine, request, StepVerbosity::Detailed)
}

pub fn compute_steps_with_verbosity(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
    verbosity: StepVerbosity,
) -> Result<SurfaceIntegralStepResult, EngineError> {
    validate_request(request)?;
    let mut result = evaluate(engine, request, false)?;
    let mut events = vec![StepEvent::new(
        "surface-integral-parameterize",
        &list(&result.surface),
        "使用给定的双参数表示积分曲面。",
        StepImportance::Key,
    )];
    events.push(StepEvent::new(
        "surface-integral-tangents",
        &format!(
            "{{{},{}}}",
            list(&result.tangent_u),
            list(&result.tangent_v)
        ),
        "分别对两个曲面参数求偏导，得到两条切向量。",
        StepImportance::Normal,
    ));
    events.push(StepEvent::new(
        "surface-integral-normal",
        &list(&result.oriented_normal),
        match request.orientation {
            SurfaceOrientation::ParameterOrder => "按参数顺序计算叉积，得到定向法向量。",
            SurfaceOrientation::Reversed => "反转参数顺序给出的法向方向。",
        },
        StepImportance::Normal,
    ));
    if let Some(area_factor) = &result.area_factor {
        events.push(StepEvent::new(
            "surface-integral-area-factor",
            area_factor,
            "计算法向量的长度，得到曲面面积微元的系数。",
            StepImportance::Normal,
        ));
    }
    events.push(StepEvent::new(
        "surface-integral-pullback",
        &list(&result.field_on_surface),
        "将参数曲面代入被积函数或向量场。",
        StepImportance::Normal,
    ));
    events.push(StepEvent::new(
        "surface-integral-integrand",
        &format!(
            "Integrate({},{},{})Integrate({},{},{})({})",
            request.parameters[1],
            request.lower[1],
            request.upper[1],
            request.parameters[0],
            request.lower[0],
            request.upper[0],
            result.integrand
        ),
        match request.kind {
            SurfaceIntegralKind::ScalarArea => "乘以面积因子，化为参数域上的二重积分。",
            SurfaceIntegralKind::VectorFlux => "与定向法向量作点积，化为参数域上的二重积分。",
        },
        StepImportance::Key,
    ));
    events.push(StepEvent::new(
        "surface-integral-inner",
        &result.inner_value,
        if result.inner_completed {
            "先对第一个曲面参数积分。"
        } else {
            "内层积分未得到解析结果。"
        },
        if result.inner_completed {
            StepImportance::Normal
        } else {
            StepImportance::Key
        },
    ));
    events.push(StepEvent::new(
        "surface-integral-result",
        &result.value,
        if result.completed {
            "再对第二个曲面参数积分，得到曲面积分。"
        } else {
            "当前积分规则未能得到完整解析结果。"
        },
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(SurfaceIntegralStepResult { result, steps })
}

fn evaluate(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
    render_value: bool,
) -> Result<SurfaceIntegralResult, EngineError> {
    let tangent_u = derivatives(&request.surface, &request.parameters[0]);
    let tangent_v = derivatives(&request.surface, &request.parameters[1]);
    let normal = cross(&tangent_u, &tangent_v);
    let oriented_normal = match request.orientation {
        SurfaceOrientation::ParameterOrder => normal,
        SurfaceOrientation::Reversed => normal.map(|component| format!("-({component})")),
    };
    let field_on_surface = request
        .field
        .iter()
        .map(|component| substitute_surface(component, &request.coordinates, &request.surface))
        .collect::<Vec<_>>();
    let area_command: String = if request.kind == SurfaceIntegralKind::ScalarArea {
        "Simplify(Sqrt(n[1]^2+n[2]^2+n[3]^2))".into()
    } else {
        "Undefined".into()
    };
    let raw_integrand: String = match request.kind {
        SurfaceIntegralKind::ScalarArea => "f[1]*a".into(),
        SurfaceIntegralKind::VectorFlux => "f[1]*n[1]+f[2]*n[2]+f[3]*n[3]".into(),
    };
    let normal_certificate = match request.orientation {
        SurfaceOrientation::ParameterOrder => {
            "And(IsZero(Simplify(n[1]-(ru[2]*rv[3]-ru[3]*rv[2]))),IsZero(Simplify(n[2]-(ru[3]*rv[1]-ru[1]*rv[3]))),IsZero(Simplify(n[3]-(ru[1]*rv[2]-ru[2]*rv[1]))))"
        }
        SurfaceOrientation::Reversed => {
            "And(IsZero(Simplify(n[1]+(ru[2]*rv[3]-ru[3]*rv[2]))),IsZero(Simplify(n[2]+(ru[3]*rv[1]-ru[1]*rv[3]))),IsZero(Simplify(n[3]+(ru[1]*rv[2]-ru[2]*rv[1]))))"
        }
    };
    let command = format!(
        "[Local(ru,rv,n,a,f,g,i,ic,o,oc,nc,c); \
         ru:=Simplify({tangent_u}); rv:=Simplify({tangent_v}); n:=Simplify({normal}); \
         a:={area_command}; f:=Simplify({field}); g:=Simplify({raw_integrand}); \
         i:=Integrate({u},{u_lower},{u_upper})g; ic:=IsFreeOf(Integrate,i); \
         If(ic,[o:=Integrate({v},{v_lower},{v_upper})i; oc:=IsFreeOf(Integrate,o);], \
               [o:=Undefined;oc:=False;]); \
         nc:={normal_certificate}; c:=IsZero(Simplify(g-({raw_integrand}))); \
         {{ru,rv,n,a,f,g,i,o,ic,oc,nc,c}};]",
        tangent_u = list(&tangent_u),
        tangent_v = list(&tangent_v),
        normal = list(&oriented_normal),
        field = list(&field_on_surface),
        u = request.parameters[0],
        u_lower = request.lower[0],
        u_upper = request.upper[0],
        v = request.parameters[1],
        v_lower = request.lower[1],
        v_upper = request.upper[1],
    );
    let data = engine.eval_expr(&command)?;
    let fields = list_expr(&data, "曲面积分结果")?;
    if fields.len() != 12 {
        return Err(EngineError::Parse("曲面积分结果字段数量异常".into()));
    }
    let tangent_u = strings(list_expr(&fields[0], "第一切向量")?);
    let tangent_v = strings(list_expr(&fields[1], "第二切向量")?);
    let oriented_normal = strings(list_expr(&fields[2], "定向法向量")?);
    let area_factor =
        (request.kind == SurfaceIntegralKind::ScalarArea).then(|| fields[3].to_string());
    let field_on_surface = strings(list_expr(&fields[4], "曲面上的场")?);
    let integrand = fields[5].to_string();
    let inner_value = fields[6].to_string();
    let value = fields[7].to_string();
    let inner_completed = boolean(&fields[8], "曲面积分内层状态")?;
    let completed = boolean(&fields[9], "曲面积分完成状态")?;
    let normal_verified = boolean(&fields[10], "曲面积分法向量证书")?;
    let integrand_verified = boolean(&fields[11], "曲面积分被积式证书")?;
    if !normal_verified || !integrand_verified {
        return Err(EngineError::Eval("曲面积分构造证书失败".into()));
    }
    let tex = if render_value {
        strip_tex_delimiters(&engine.render_tex_batch(std::slice::from_ref(&value))?[0])
    } else {
        String::new()
    };
    Ok(SurfaceIntegralResult {
        kind: request.kind,
        orientation: request.orientation,
        surface: request.surface.clone(),
        tangent_u,
        tangent_v,
        oriented_normal,
        area_factor,
        field_on_surface,
        integrand,
        inner_value,
        value,
        inner_completed,
        completed,
        normal_verified,
        integrand_verified,
        tex,
    })
}

fn validate_request(request: &SurfaceIntegralRequest) -> Result<(), EngineError> {
    let mut variables = BTreeSet::new();
    for coordinate in &request.coordinates {
        validate_symbol(coordinate, "空间坐标")?;
        if !variables.insert(coordinate) {
            return Err(EngineError::InvalidInput("空间坐标不能重复".into()));
        }
    }
    for parameter in &request.parameters {
        validate_symbol(parameter, "曲面参数")?;
        if !variables.insert(parameter) {
            return Err(EngineError::InvalidInput(
                "曲面参数必须互异且不能与空间坐标相同".into(),
            ));
        }
    }
    let expected_fields = match request.kind {
        SurfaceIntegralKind::ScalarArea => 1,
        SurfaceIntegralKind::VectorFlux => 3,
    };
    if request.field.len() != expected_fields {
        return Err(EngineError::InvalidInput(
            "被积函数或向量场的分量数量与曲面积分类型不匹配".into(),
        ));
    }
    for field in &request.field {
        let analysis = analyze_expression(field, "曲面积分场分量")?;
        if request
            .parameters
            .iter()
            .any(|parameter| analysis.symbols.contains(parameter))
        {
            return Err(EngineError::InvalidInput(
                "曲面积分场分量不能直接依赖曲面参数".into(),
            ));
        }
    }
    for component in &request.surface {
        let analysis = analyze_expression(component, "参数曲面分量")?;
        if request
            .coordinates
            .iter()
            .any(|coordinate| analysis.symbols.contains(coordinate))
        {
            return Err(EngineError::InvalidInput(
                "参数曲面分量不能依赖被替换的空间坐标".into(),
            ));
        }
    }
    for (index, parameter) in request.parameters.iter().enumerate() {
        for (bound, label) in [
            (&request.lower[index], "参数下限"),
            (&request.upper[index], "参数上限"),
        ] {
            let analysis = analyze_expression(bound, label)?;
            if request
                .parameters
                .iter()
                .any(|candidate| analysis.symbols.contains(candidate))
            {
                return Err(EngineError::InvalidInput(format!(
                    "{parameter} 的矩形参数域端点不能依赖曲面参数"
                )));
            }
        }
    }
    Ok(())
}

fn derivatives(surface: &[String; 3], parameter: &str) -> [String; 3] {
    surface
        .each_ref()
        .map(|component| format!("Deriv({parameter})({component})"))
}

fn cross(left: &[String; 3], right: &[String; 3]) -> [String; 3] {
    [
        format!("({})*({})-({})*({})", left[1], right[2], left[2], right[1]),
        format!("({})*({})-({})*({})", left[2], right[0], left[0], right[2]),
        format!("({})*({})-({})*({})", left[0], right[1], left[1], right[0]),
    ]
}

fn substitute_surface(
    expression: &str,
    coordinates: &[String; 3],
    surface: &[String; 3],
) -> String {
    coordinates.iter().zip(surface).fold(
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

    fn request(kind: SurfaceIntegralKind, field: &[&str]) -> SurfaceIntegralRequest {
        SurfaceIntegralRequest {
            kind,
            orientation: SurfaceOrientation::ParameterOrder,
            field: field.iter().map(|value| (*value).into()).collect(),
            coordinates: ["x".into(), "y".into(), "z".into()],
            surface: ["u".into(), "v".into(), "0".into()],
            parameters: ["u".into(), "v".into()],
            lower: ["0".into(), "0".into()],
            upper: ["2".into(), "3".into()],
        }
    }

    #[test]
    fn computes_plane_area_and_oriented_flux() {
        let mut engine = RustEngine::spawn().unwrap();
        let area = compute(
            &mut engine,
            &request(SurfaceIntegralKind::ScalarArea, &["1"]),
        )
        .unwrap();
        assert!(area.completed && area.normal_verified && area.integrand_verified);
        assert_eq!(area.area_factor.as_deref(), Some("1"));
        assert_eq!(area.value, "6");

        let flux_request = request(SurfaceIntegralKind::VectorFlux, &["0", "0", "1"]);
        let flux = compute(&mut engine, &flux_request).unwrap();
        assert!(flux.completed && flux.normal_verified && flux.integrand_verified);
        assert_eq!(flux.value, "6");
        assert_eq!(flux.area_factor, None);

        let mut reversed = flux_request;
        reversed.orientation = SurfaceOrientation::Reversed;
        let reversed_flux = compute(&mut engine, &reversed).unwrap();
        assert_eq!(reversed_flux.value, "-6");

        let mut graph = request(SurfaceIntegralKind::ScalarArea, &["1"]);
        graph.surface[2] = "u+v".into();
        graph.upper = ["1".into(), "1".into()];
        let graph_area = compute(&mut engine, &graph).unwrap();
        assert!(graph_area.completed && graph_area.normal_verified);
        assert_eq!(graph_area.oriented_normal, ["-1", "-1", "1"]);
        assert_eq!(
            engine
                .eval(&format!("IsZero(({})-Sqrt(3))", graph_area.value))
                .unwrap()
                .expr
                .to_string(),
            "True"
        );
    }

    #[test]
    fn emits_filtered_steps_in_one_tex_batch() {
        let request = request(SurfaceIntegralKind::ScalarArea, &["x+y"]);
        let mut engine = CountingEngine::spawn();
        let detailed = compute_steps(&mut engine, &request).unwrap();
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);
        assert!(detailed
            .steps
            .iter()
            .any(|step| step.rule == "surface-integral-normal"));

        engine.reset_counts();
        let concise =
            compute_steps_with_verbosity(&mut engine, &request, StepVerbosity::Concise).unwrap();
        assert!(concise.steps.len() < detailed.steps.len());
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
    }

    #[test]
    fn reports_unresolved_integrals_and_rejects_invalid_requests() {
        let mut engine = RustEngine::spawn().unwrap();
        let unresolved = compute(
            &mut engine,
            &request(SurfaceIntegralKind::ScalarArea, &["Sin(x^x)"]),
        )
        .unwrap();
        assert!(!unresolved.completed);

        let mut invalid = request(SurfaceIntegralKind::VectorFlux, &["1"]);
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(SurfaceIntegralKind::ScalarArea, &["u"]);
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(SurfaceIntegralKind::ScalarArea, &["1"]);
        invalid.surface[0] = "x+u".into();
        assert!(compute(&mut engine, &invalid).is_err());
        invalid = request(SurfaceIntegralKind::ScalarArea, &["1"]);
        invalid.surface[0] = "u);Echo(1);(u".into();
        assert!(compute(&mut engine, &invalid).is_err());

        assert_eq!(
            engine.eval("2+2").unwrap().expr.to_string(),
            "4",
            "invalid requests must not poison the engine"
        );
    }
}
