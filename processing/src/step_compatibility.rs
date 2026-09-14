//! Legacy/product step entry points kept outside mathematical domain modules.
//!
//! These functions consume authoritative domain computations and only project
//! their rule traces into teaching-step DTOs.

use crate::derivatives::derivative_computation;
use crate::engine::{Engine, EngineError};
use crate::input::{validate_expression, validate_symbol};
use crate::limits::{limit_computation, LimitDirection};
use crate::steps::{Step, StepVerbosity};

fn derivative_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let computation = derivative_computation(engine, expression, variable, order)?;
    crate::steps::render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .expect("derivative always emits trace"),
        verbosity,
    )
}

pub fn derive_steps(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
) -> Result<Vec<Step>, EngineError> {
    derivative_steps_with_verbosity(engine, expression, variable, 1, StepVerbosity::Detailed)
}

pub fn derive_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    derivative_steps_with_verbosity(engine, expression, variable, 1, verbosity)
}

pub fn derive_steps_order(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
) -> Result<Vec<Step>, EngineError> {
    derivative_steps_with_verbosity(engine, expression, variable, order, StepVerbosity::Detailed)
}

pub fn derive_steps_order_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    order: u32,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    derivative_steps_with_verbosity(engine, expression, variable, order, verbosity)
}

pub fn limit_steps(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
) -> Result<Vec<Step>, EngineError> {
    limit_steps_with_verbosity(
        engine,
        expression,
        variable,
        at,
        direction,
        StepVerbosity::Detailed,
    )
}

pub fn limit_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    at: &str,
    direction: LimitDirection,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expression(expression, "极限表达式")?;
    validate_expression(at, "趋近点")?;
    validate_symbol(variable, "极限变量")?;
    let computation = limit_computation(engine, expression, variable, at, direction)?;
    crate::steps::render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .expect("limit computation always records its rule trace"),
        verbosity,
    )
}

use crate::integrals::{
    antiderivative_family, definite_integral_computation_with_options, evaluate_integral_rules,
    AntiderivativeFamily, DefiniteIntegralRequest, IntegralEvaluation,
};
use crate::quadrature::QuadratureOptions;
use crate::semantic_core::RuleImportance;
use crate::steps::{render_events, render_rule_trace, StepEvent, StepImportance, StepKind};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AntiderivativeStepResult {
    pub result: AntiderivativeFamily,
    pub steps: Vec<Step>,
}

pub fn derive_integrals_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    Ok(integral_compatibility_evaluation(engine, expression, variable, verbosity)?.1)
}

fn integral_compatibility_evaluation(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    verbosity: StepVerbosity,
) -> Result<(IntegralEvaluation, Vec<Step>), EngineError> {
    validate_expression(expression, "表达式")?;
    validate_symbol(variable, "积分变量")?;
    let evaluation = evaluate_integral_rules(
        engine,
        &format!("StepsI'Computation({expression},{variable})"),
    )?;
    let events = evaluation
        .emissions
        .iter()
        .map(|emission| {
            StepEvent::new(
                &emission.rule,
                &emission.expression,
                &emission.explanation,
                match emission.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            )
        })
        .collect();
    let steps = render_events(engine, events, verbosity)?;
    Ok((evaluation, steps))
}

pub fn derive_integrals(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_integrals_with_verbosity(engine, expression, variable, StepVerbosity::Detailed)
}

pub fn derive_antiderivative_family_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    arbitrary_constant: String,
    verbosity: StepVerbosity,
) -> Result<AntiderivativeStepResult, EngineError> {
    let (evaluation, mut steps) =
        integral_compatibility_evaluation(engine, expression, variable, verbosity)?;
    let representative = evaluation.result;
    let representative_tex = engine
        .render_syntax_tex_batch(std::slice::from_ref(&representative))?
        .pop()
        .map(|tex| crate::input::strip_tex_delimiters(&tex))
        .ok_or_else(|| EngineError::Parse("不定积分代表元缺少 TeX 投影".into()))?;
    let result = antiderivative_family(
        representative,
        representative_tex,
        variable,
        arbitrary_constant,
    );
    steps.push(Step {
        kind: StepKind::EquivalentTransformation,
        before_expr: None,
        before_tex: None,
        rule: "antiderivative-family".into(),
        expr: result.expression.clone(),
        why: "加入任意常数，表示全部原函数。".into(),
        tex: result.tex.clone(),
        importance: StepImportance::Key,
    });
    Ok(AntiderivativeStepResult { result, steps })
}

pub fn derive_definite(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(
        engine,
        expression,
        variable,
        lower,
        upper,
        None,
        StepVerbosity::Detailed,
    )
}

pub fn derive_definite_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(engine, expression, variable, lower, upper, None, verbosity)
}

pub fn derive_definite_with_options(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    options: &QuadratureOptions,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(
        engine,
        expression,
        variable,
        lower,
        upper,
        Some(options),
        StepVerbosity::Detailed,
    )
}

#[allow(clippy::too_many_arguments)]
fn derive_definite_configured(
    engine: &mut dyn Engine,
    expression: &str,
    variable: &str,
    lower: &str,
    upper: &str,
    numeric_fallback: Option<&QuadratureOptions>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expression(expression, "被积表达式")?;
    validate_expression(lower, "下限")?;
    validate_expression(upper, "上限")?;
    validate_symbol(variable, "积分变量")?;
    let computation = definite_integral_computation_with_options(
        engine,
        expression,
        &DefiniteIntegralRequest {
            variable: variable.into(),
            lower: lower.into(),
            upper: upper.into(),
        },
        numeric_fallback,
    )?;
    render_rule_trace(
        engine,
        computation
            .trace
            .as_ref()
            .ok_or_else(|| EngineError::Eval("定积分计算缺少规则轨迹".into()))?,
        verbosity,
    )
}

use crate::series::{
    infinite_series_evaluation, power_series_evaluation, InfiniteSeriesResult, PowerSeriesResult,
    SeriesRuleEmission,
};

#[derive(Debug, Clone, Serialize)]
pub struct InfiniteSeriesStepResult {
    pub result: InfiniteSeriesResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerSeriesStepResult {
    pub result: PowerSeriesResult,
    pub steps: Vec<Step>,
}

pub fn infinite_series_steps(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
) -> Result<InfiniteSeriesStepResult, EngineError> {
    infinite_series_steps_with_verbosity(engine, term, variable, from, StepVerbosity::Detailed)
}

pub fn infinite_series_steps_with_verbosity(
    engine: &mut dyn Engine,
    term: &str,
    variable: &str,
    from: &str,
    verbosity: StepVerbosity,
) -> Result<InfiniteSeriesStepResult, EngineError> {
    let (mut result, emissions) = infinite_series_evaluation(engine, term, variable, from)?;
    let steps = render_series_emissions(engine, emissions, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(InfiniteSeriesStepResult { result, steps })
}

pub fn power_series_steps(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
) -> Result<PowerSeriesStepResult, EngineError> {
    power_series_steps_with_verbosity(
        engine,
        coefficient,
        index,
        variable,
        center,
        StepVerbosity::Detailed,
    )
}

pub fn power_series_steps_with_verbosity(
    engine: &mut dyn Engine,
    coefficient: &str,
    index: &str,
    variable: &str,
    center: &str,
    verbosity: StepVerbosity,
) -> Result<PowerSeriesStepResult, EngineError> {
    let (mut result, emissions) =
        power_series_evaluation(engine, coefficient, index, variable, center)?;
    let steps = render_series_emissions(engine, emissions, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(PowerSeriesStepResult { result, steps })
}

fn render_series_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<SeriesRuleEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|emission| {
                StepEvent::new(
                    &emission.rule,
                    &emission.expression,
                    &emission.explanation,
                    match emission.importance {
                        RuleImportance::Routine => StepImportance::Routine,
                        RuleImportance::Normal => StepImportance::Normal,
                        RuleImportance::Key => StepImportance::Key,
                    },
                )
            })
            .collect(),
        verbosity,
    )
}

use crate::multiple_integrals::{
    double_integral_evaluation, polar_integral_evaluation, triple_integral_evaluation,
    DoubleIntegralResult, IntegralBound, MultipleIntegralRuleEmission, PolarIntegralResult,
    PolarRegion, TripleIntegralResult,
};

#[derive(Debug, Clone, Serialize)]
pub struct DoubleIntegralStepResult {
    pub result: DoubleIntegralResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TripleIntegralStepResult {
    pub result: TripleIntegralResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PolarIntegralStepResult {
    pub result: PolarIntegralResult,
    pub steps: Vec<Step>,
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
    let (mut result, emissions) =
        polar_integral_evaluation(engine, expression, x, y, radius, angle, region)?;
    let steps = render_multiple_integral_emissions(engine, emissions, verbosity)?;
    result.integral.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(PolarIntegralStepResult { result, steps })
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
    let (mut result, emissions) =
        double_integral_evaluation(engine, expression, inner_bound, outer_bound)?;
    let steps = render_multiple_integral_emissions(engine, emissions, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(DoubleIntegralStepResult { result, steps })
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
    let (mut result, emissions) =
        triple_integral_evaluation(engine, expression, inner, middle, outer)?;
    let steps = render_multiple_integral_emissions(engine, emissions, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(TripleIntegralStepResult { result, steps })
}

fn render_multiple_integral_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<MultipleIntegralRuleEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|emission| {
                StepEvent::new(
                    &emission.rule,
                    &emission.expression,
                    &emission.explanation,
                    match emission.importance {
                        RuleImportance::Routine => StepImportance::Routine,
                        RuleImportance::Normal => StepImportance::Normal,
                        RuleImportance::Key => StepImportance::Key,
                    },
                )
            })
            .collect(),
        verbosity,
    )
}

use crate::equations::{equation_evaluation, EquationRuleEmission, SolveResult};

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
    let (result, emissions) = equation_evaluation(engine, &[equation], &[variable])?;
    let steps = render_equation_emissions(engine, emissions, verbosity)?;
    Ok(EquationStepResult { result, steps })
}

pub fn solve_system_steps(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
) -> Result<EquationStepResult, EngineError> {
    solve_system_steps_with_verbosity(engine, equations, variables, StepVerbosity::Detailed)
}

pub fn solve_system_steps_with_verbosity(
    engine: &mut dyn Engine,
    equations: &[&str],
    variables: &[&str],
    verbosity: StepVerbosity,
) -> Result<EquationStepResult, EngineError> {
    let (result, emissions) = equation_evaluation(engine, equations, variables)?;
    let steps = render_equation_emissions(engine, emissions, verbosity)?;
    Ok(EquationStepResult { result, steps })
}

fn render_equation_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<EquationRuleEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|emission| StepEvent {
                rule: emission.rule,
                expr: emission.expression,
                why: emission.explanation,
                importance: match emission.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            })
            .collect(),
        verbosity,
    )
}

use crate::ode::{
    ode_rule_emissions, solve_internal as solve_ode_internal, InitialCondition, OdeEvent, OdeResult,
};

#[derive(Debug, Clone, Serialize)]
pub struct OdeStepResult {
    pub result: OdeResult,
    pub steps: Vec<Step>,
}

pub fn ode_solve_steps(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
) -> Result<OdeStepResult, EngineError> {
    ode_solve_steps_with_verbosity(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        StepVerbosity::Detailed,
    )
}

pub fn ode_solve_steps_with_verbosity(
    engine: &mut dyn Engine,
    equation: &str,
    independent: &str,
    dependent: &str,
    initial_conditions: &[InitialCondition<'_>],
    verbosity: StepVerbosity,
) -> Result<OdeStepResult, EngineError> {
    let (result, solver_events) = solve_ode_internal(
        engine,
        equation,
        independent,
        dependent,
        initial_conditions,
        true,
    )?;
    let events = ode_rule_emissions(
        equation,
        independent,
        dependent,
        initial_conditions,
        &result,
        solver_events,
    );
    let steps = render_ode_emissions(engine, events, verbosity)?;
    Ok(OdeStepResult { result, steps })
}

fn render_ode_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<OdeEvent>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|event| StepEvent {
                rule: event.rule,
                expr: event.expr,
                why: event.explanation,
                importance: match event.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            })
            .collect(),
        verbosity,
    )
}

use crate::linear_algebra::{
    linear_structure_evaluation, LinearStructureEmission, LinearStructureResult, RowOperation,
};

#[derive(Debug, Clone, Serialize)]
pub struct LinearStructureStepResult {
    pub result: LinearStructureResult,
    pub operations: Vec<RowOperation>,
    pub steps: Vec<Step>,
}

pub fn linear_structure_steps(
    engine: &mut dyn Engine,
    matrix: &str,
) -> Result<LinearStructureStepResult, EngineError> {
    linear_structure_steps_with_verbosity(engine, matrix, StepVerbosity::Detailed)
}

pub fn linear_structure_steps_with_verbosity(
    engine: &mut dyn Engine,
    matrix: &str,
    verbosity: StepVerbosity,
) -> Result<LinearStructureStepResult, EngineError> {
    let (mut result, operations, emissions) = linear_structure_evaluation(engine, matrix)?;
    let steps = render_linear_structure_emissions(engine, emissions, verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(LinearStructureStepResult {
        result,
        operations,
        steps,
    })
}

fn render_linear_structure_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<LinearStructureEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|event| StepEvent {
                rule: event.rule,
                expr: event.expression,
                why: event.explanation,
                importance: match event.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            })
            .collect(),
        verbosity,
    )
}

use crate::extrema::{
    analyze_internal as analyze_extrema_internal,
    analyze_lagrange_internal as analyze_extrema_lagrange_internal, extrema_step_events,
    lagrange_step_events, validate_lagrange_request, validate_request as validate_extrema_request,
    ExtremaEmission, ExtremaResult, LagrangeResult,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExtremaStepResult {
    pub result: ExtremaResult,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LagrangeStepResult {
    pub result: LagrangeResult,
    pub steps: Vec<Step>,
}

pub fn extrema_analyze_steps(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
) -> Result<ExtremaStepResult, EngineError> {
    extrema_analyze_steps_with_verbosity(engine, expression, x, y, StepVerbosity::Detailed)
}

pub fn extrema_analyze_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    x: &str,
    y: &str,
    verbosity: StepVerbosity,
) -> Result<ExtremaStepResult, EngineError> {
    validate_extrema_request(expression, x, y)?;
    let mut result = analyze_extrema_internal(engine, expression, x, y, false)?;
    let steps = render_extrema_emissions(engine, extrema_step_events(&result), verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(ExtremaStepResult { result, steps })
}

pub fn extrema_analyze_lagrange_steps(
    engine: &mut dyn Engine,
    expression: &str,
    constraint: &str,
    x: &str,
    y: &str,
) -> Result<LagrangeStepResult, EngineError> {
    extrema_analyze_lagrange_steps_with_verbosity(
        engine,
        expression,
        constraint,
        x,
        y,
        StepVerbosity::Detailed,
    )
}

pub fn extrema_analyze_lagrange_steps_with_verbosity(
    engine: &mut dyn Engine,
    expression: &str,
    constraint: &str,
    x: &str,
    y: &str,
    verbosity: StepVerbosity,
) -> Result<LagrangeStepResult, EngineError> {
    validate_lagrange_request(expression, constraint, x, y)?;
    let mut result =
        analyze_extrema_lagrange_internal(engine, expression, constraint, x, y, false)?;
    let steps = render_extrema_emissions(engine, lagrange_step_events(&result), verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(LagrangeStepResult { result, steps })
}

fn render_extrema_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<ExtremaEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|event| StepEvent {
                rule: event.rule,
                expr: event.expr,
                why: event.why,
                importance: match event.importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            })
            .collect(),
        verbosity,
    )
}

#[cfg(test)]
use crate::line_integrals::{
    evaluate as evaluate_line_integral, line_integral_events,
    validate_request as validate_line_integral_request, LineIntegralEmission, LineIntegralRequest,
    LineIntegralResult,
};
#[cfg(test)]
use crate::surface_integrals::{
    evaluate as evaluate_surface_integral, surface_integral_events,
    validate_request as validate_surface_integral_request, SurfaceIntegralEmission,
    SurfaceIntegralRequest, SurfaceIntegralResult,
};

#[derive(Debug, Clone, Serialize)]
#[cfg(test)]
pub(crate) struct LineIntegralStepResult {
    pub result: LineIntegralResult,
    pub steps: Vec<Step>,
}

#[cfg(test)]
pub(crate) fn line_integral_compute(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
) -> Result<LineIntegralResult, EngineError> {
    validate_line_integral_request(request)?;
    evaluate_line_integral(engine, request, true)
}

#[cfg(test)]
pub(crate) fn line_integral_compute_steps(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
) -> Result<LineIntegralStepResult, EngineError> {
    line_integral_compute_steps_with_verbosity(engine, request, StepVerbosity::Detailed)
}

#[cfg(test)]
pub(crate) fn line_integral_compute_steps_with_verbosity(
    engine: &mut dyn Engine,
    request: &LineIntegralRequest,
    verbosity: StepVerbosity,
) -> Result<LineIntegralStepResult, EngineError> {
    validate_line_integral_request(request)?;
    let mut result = evaluate_line_integral(engine, request, false)?;
    let steps =
        render_line_integral_emissions(engine, line_integral_events(&result, request), verbosity)?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(LineIntegralStepResult { result, steps })
}

#[cfg(test)]
fn render_line_integral_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<LineIntegralEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_domain_emissions(
        engine,
        emissions
            .into_iter()
            .map(|event| (event.rule, event.expr, event.why, event.importance)),
        verbosity,
    )
}

#[derive(Debug, Clone, Serialize)]
#[cfg(test)]
pub(crate) struct SurfaceIntegralStepResult {
    pub result: SurfaceIntegralResult,
    pub steps: Vec<Step>,
}

#[cfg(test)]
pub(crate) fn surface_integral_compute(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
) -> Result<SurfaceIntegralResult, EngineError> {
    validate_surface_integral_request(request)?;
    evaluate_surface_integral(engine, request, true)
}

#[cfg(test)]
pub(crate) fn surface_integral_compute_steps(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
) -> Result<SurfaceIntegralStepResult, EngineError> {
    surface_integral_compute_steps_with_verbosity(engine, request, StepVerbosity::Detailed)
}

#[cfg(test)]
pub(crate) fn surface_integral_compute_steps_with_verbosity(
    engine: &mut dyn Engine,
    request: &SurfaceIntegralRequest,
    verbosity: StepVerbosity,
) -> Result<SurfaceIntegralStepResult, EngineError> {
    validate_surface_integral_request(request)?;
    let mut result = evaluate_surface_integral(engine, request, false)?;
    let steps = render_surface_integral_emissions(
        engine,
        surface_integral_events(&result, request),
        verbosity,
    )?;
    result.tex = steps
        .last()
        .map(|step| step.tex.clone())
        .unwrap_or_default();
    Ok(SurfaceIntegralStepResult { result, steps })
}

#[cfg(test)]
fn render_surface_integral_emissions(
    engine: &mut dyn Engine,
    emissions: Vec<SurfaceIntegralEmission>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_domain_emissions(
        engine,
        emissions
            .into_iter()
            .map(|event| (event.rule, event.expr, event.why, event.importance)),
        verbosity,
    )
}

fn render_domain_emissions(
    engine: &mut dyn Engine,
    emissions: impl IntoIterator<Item = (String, String, String, RuleImportance)>,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    render_events(
        engine,
        emissions
            .into_iter()
            .map(|(rule, expr, why, importance)| StepEvent {
                rule,
                expr,
                why,
                importance: match importance {
                    RuleImportance::Routine => StepImportance::Routine,
                    RuleImportance::Normal => StepImportance::Normal,
                    RuleImportance::Key => StepImportance::Key,
                },
            })
            .collect(),
        verbosity,
    )
}

use crate::improper_integrals::{defined_integral_rule_events, DefinedIntegralEmission};
use crate::objects::DefinedObjectResult;

pub fn defined_integral_steps(
    engine: &mut dyn Engine,
    result: &DefinedObjectResult,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let emissions: Vec<DefinedIntegralEmission> = defined_integral_rule_events(result);
    render_domain_emissions(
        engine,
        emissions
            .into_iter()
            .map(|event| (event.rule, event.expr, event.why, event.importance)),
        verbosity,
    )
}
