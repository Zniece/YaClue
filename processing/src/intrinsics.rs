//! Bounded lowering from defined objects to native symbolic intrinsics.

use serde::Serialize;
use std::rc::Rc;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

use crate::binding;
use crate::engine::{Engine, EngineError};
use crate::improper_integrals::ImproperIntegralRequest;
use crate::input::strip_tex_delimiters;
use crate::objects::DefinedObjectKind;
use crate::protocol::{Condition, ConditionSet};

const MAX_KERNEL_FACTORS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntrinsicKind {
    Gamma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFeature {
    PositiveHalfLineExponentialKernel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IntrinsicSignature {
    pub intrinsic: IntrinsicKind,
    pub source: DefinedObjectKind,
    pub feature: SourceFeature,
}

pub const INTRINSIC_SIGNATURES: &[IntrinsicSignature] = &[IntrinsicSignature {
    intrinsic: IntrinsicKind::Gamma,
    source: DefinedObjectKind::ImproperIntegral,
    feature: SourceFeature::PositiveHalfLineExponentialKernel,
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoweringVerification {
    StructuralKernelMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleBinding {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoweringCertificate {
    pub rule: String,
    pub source_kind: DefinedObjectKind,
    pub intrinsic: IntrinsicKind,
    pub bindings: Vec<RuleBinding>,
    pub conditions: ConditionSet,
    pub verification: LoweringVerification,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntrinsicLoweringResult {
    pub intrinsic: IntrinsicKind,
    pub source: String,
    pub target: String,
    pub value: String,
    pub tex: String,
    pub conditions: ConditionSet,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<LoweringCertificate>,
}

struct GammaMatch {
    argument: String,
    scale: String,
    constant: String,
}

/// A miss is an ordinary `None`; the caller then evaluates the original
/// object. Direct native calls never enter this registry.
pub fn try_lower_improper_integral(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
) -> Result<Option<IntrinsicLoweringResult>, EngineError> {
    try_lower_improper_integral_configured(engine, request, None, false)
}

#[cfg(test)]
pub(crate) fn try_lower_improper_integral_with_certificate(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
) -> Result<Option<IntrinsicLoweringResult>, EngineError> {
    try_lower_improper_integral_configured(engine, request, None, true)
}

pub(crate) fn try_lower_improper_integral_ast_with_certificate(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    expression: &Rc<LispObject>,
) -> Result<Option<IntrinsicLoweringResult>, EngineError> {
    try_lower_improper_integral_configured(engine, request, Some(expression), true)
}

fn try_lower_improper_integral_configured(
    engine: &mut dyn Engine,
    request: &ImproperIntegralRequest,
    expression: Option<&Rc<LispObject>>,
    include_certificate: bool,
) -> Result<Option<IntrinsicLoweringResult>, EngineError> {
    let Some(signature) = select_signature(request) else {
        return Ok(None);
    };
    debug_assert_eq!(signature.intrinsic, IntrinsicKind::Gamma);
    let parsed_expression;
    let expression = match expression {
        Some(expression) => expression,
        None => {
            parsed_expression = crate::semantic_core::parse_engine_expression(&request.expression)?
                .raw_expression();
            &parsed_expression
        }
    };
    let Some(matched) = match_gamma_kernel_ast(request, expression)? else {
        return Ok(None);
    };
    if nonpositive_numeric(&matched.argument) || nonpositive_numeric(&matched.scale) {
        return Ok(None);
    }

    let mut required = Vec::new();
    if !positive_numeric(&matched.argument) {
        required.push(Condition::RealPartPositive {
            expression: matched.argument.clone(),
        });
    }
    if matched.scale != "1" && !positive_numeric(&matched.scale) {
        required.push(Condition::Positive {
            expression: matched.scale.clone(),
        });
    }
    let conditions = ConditionSet::new(required)?;
    let gamma = format!("Gamma({})", grouped(&matched.argument));
    let scaled = if matched.scale == "1" {
        gamma
    } else {
        format!("({gamma})/(({})^({}))", matched.scale, matched.argument)
    };
    let target = if matched.constant == "1" {
        scaled
    } else {
        format!("({})*({scaled})", matched.constant)
    };
    let evaluated = engine.eval(&target)?;
    let value = evaluated.expr.to_string();
    let tex = strip_tex_delimiters(&evaluated.tex);
    let certificate = include_certificate.then(|| LoweringCertificate {
        rule: "gamma-euler-kernel".into(),
        source_kind: signature.source,
        intrinsic: signature.intrinsic,
        bindings: vec![
            RuleBinding {
                name: "parameter".into(),
                value: matched.argument,
            },
            RuleBinding {
                name: "scale".into(),
                value: matched.scale,
            },
            RuleBinding {
                name: "constant".into(),
                value: matched.constant,
            },
            RuleBinding {
                name: "bound_variable".into(),
                value: request.variable.clone(),
            },
        ],
        conditions: conditions.clone(),
        verification: LoweringVerification::StructuralKernelMatch,
    });
    Ok(Some(IntrinsicLoweringResult {
        intrinsic: IntrinsicKind::Gamma,
        source: source_expression(request),
        target,
        value,
        tex,
        conditions,
        certificate,
    }))
}

fn cheap_gamma_feature(request: &ImproperIntegralRequest) -> bool {
    request.lower.trim() == "0"
        && request.upper.trim() == "Infinity"
        && request.singular_points.is_empty()
        && request.expression.contains("Exp")
}

fn select_signature(request: &ImproperIntegralRequest) -> Option<&'static IntrinsicSignature> {
    // Feature selection is constant-time and points directly at its registry
    // slot. It never walks unrelated recognizers.
    cheap_gamma_feature(request).then_some(&INTRINSIC_SIGNATURES[0])
}

fn match_gamma_kernel_ast(
    request: &ImproperIntegralRequest,
    expression: &Rc<LispObject>,
) -> Result<Option<GammaMatch>, EngineError> {
    let (mut factors, denominator_variables) =
        multiplicative_factors(expression, &request.variable)?;
    if factors.len() > MAX_KERNEL_FACTORS || denominator_variables > 1 {
        return Ok(None);
    }
    let mut exponential = None;
    let mut power = None;
    let mut constants = Vec::new();
    for factor in factors.drain(..) {
        if let Some(scale) = exponential_scale(&factor, &request.variable)? {
            if exponential.replace(scale).is_some() {
                return Ok(None);
            }
        } else if let Some(exponent) = variable_power(&factor, &request.variable)? {
            if power.replace(exponent).is_some() {
                return Ok(None);
            }
        } else if free_of_ast(&factor, &request.variable) {
            constants.push(ast_source(&factor));
        } else {
            return Ok(None);
        }
    }
    let Some(scale) = exponential else {
        return Ok(None);
    };
    let mut exponent = power.unwrap_or_else(|| "0".into());
    if denominator_variables == 1 {
        exponent = format!("({exponent})-1");
    }
    let argument = gamma_argument(&exponent)?;
    if !free_of(&argument, &request.variable)? {
        return Ok(None);
    }
    Ok(Some(GammaMatch {
        argument,
        scale,
        constant: if constants.is_empty() {
            "1".into()
        } else {
            constants
                .into_iter()
                .map(|factor| grouped(&factor))
                .collect::<Vec<_>>()
                .join("*")
        },
    }))
}

fn multiplicative_factors(
    expression: &Rc<LispObject>,
    variable: &str,
) -> Result<(Vec<Rc<LispObject>>, usize), EngineError> {
    let Some((head, arguments)) = call_parts(expression) else {
        return Ok((vec![expression.clone()], 0));
    };
    match (head.as_str(), arguments.as_slice()) {
        ("*", [left, right]) => {
            let (mut left_factors, left_divisors) = multiplicative_factors(left, variable)?;
            let (right_factors, right_divisors) = multiplicative_factors(right, variable)?;
            left_factors.extend(right_factors);
            Ok((left_factors, left_divisors + right_divisors))
        }
        ("/", [numerator, denominator]) if is_symbol(denominator, variable) => {
            let (factors, divisors) = multiplicative_factors(numerator, variable)?;
            Ok((factors, divisors + 1))
        }
        ("/", [numerator, denominator]) if free_of_ast(denominator, variable) => {
            let (mut factors, divisors) = multiplicative_factors(numerator, variable)?;
            let reciprocal = crate::semantic_core::parse_engine_expression(&format!(
                "1/({})",
                ast_source(denominator)
            ))?
            .raw_expression();
            factors.push(reciprocal);
            Ok((factors, divisors))
        }
        _ => Ok((vec![expression.clone()], 0)),
    }
}

fn exponential_scale(
    factor: &Rc<LispObject>,
    variable: &str,
) -> Result<Option<String>, EngineError> {
    let Some((head, arguments)) = call_parts(factor) else {
        return Ok(None);
    };
    let ("Exp", [argument]) = (head.as_str(), arguments.as_slice()) else {
        return Ok(None);
    };
    let Some((negative_head, negative_arguments)) = call_parts(argument) else {
        return Ok(None);
    };
    let ("-", [kernel]) = (negative_head.as_str(), negative_arguments.as_slice()) else {
        return Ok(None);
    };
    if is_symbol(kernel, variable) {
        return Ok(Some("1".into()));
    }
    let Some((product_head, product_arguments)) = call_parts(kernel) else {
        return Ok(None);
    };
    let ("*", [left, right]) = (product_head.as_str(), product_arguments.as_slice()) else {
        return Ok(None);
    };
    if is_symbol(left, variable) && free_of_ast(right, variable) {
        Ok(Some(ast_source(right)))
    } else if is_symbol(right, variable) && free_of_ast(left, variable) {
        Ok(Some(ast_source(left)))
    } else {
        Ok(None)
    }
}

fn variable_power(factor: &Rc<LispObject>, variable: &str) -> Result<Option<String>, EngineError> {
    if is_symbol(factor, variable) {
        return Ok(Some("1".into()));
    }
    let Some((head, arguments)) = call_parts(factor) else {
        return Ok(None);
    };
    match (head.as_str(), arguments.as_slice()) {
        ("^", [base, exponent]) if is_symbol(base, variable) => Ok(Some(ast_source(exponent))),
        _ => Ok(None),
    }
}

fn gamma_argument(exponent: &str) -> Result<String, EngineError> {
    if let Ok(value) = exponent.trim_matches(['(', ')', ' ']).parse::<f64>() {
        if value.is_finite() {
            return Ok((value + 1.0).to_string());
        }
    }
    let parsed = crate::semantic_core::parse_engine_expression(exponent)?.raw_expression();
    if let Some((head, arguments)) = call_parts(&parsed) {
        if let ("-", [argument, one]) = (head.as_str(), arguments.as_slice()) {
            if is_symbol(one, "1") || one.number_string().as_deref() == Some("1") {
                return Ok(ast_source(argument));
            }
        }
    }
    Ok(format!("({exponent})+1"))
}

fn call_parts(expression: &Rc<LispObject>) -> Option<(String, Vec<&Rc<LispObject>>)> {
    let ObjectKind::Sublist(first) = &expression.kind else {
        return None;
    };
    let nodes = spine_refs(first).collect::<Vec<_>>();
    let head = nodes.first()?.atom_string()?.to_string();
    Some((head, nodes.into_iter().skip(1).collect()))
}

fn is_symbol(expression: &Rc<LispObject>, symbol: &str) -> bool {
    expression
        .atom_string()
        .is_some_and(|value| value.as_ref() == symbol)
}

fn ast_source(expression: &Rc<LispObject>) -> String {
    crate::input::with_parse_env(|env| yacas_rs::printer::infix_print(env, expression))
}

fn free_of_ast(expression: &Rc<LispObject>, variable: &str) -> bool {
    !binding::analyze_tree(expression)
        .free_symbols
        .contains(variable)
}

fn free_of(expression: &str, variable: &str) -> Result<bool, EngineError> {
    Ok(!binding::analyze(expression)?
        .free_symbols
        .iter()
        .any(|symbol| symbol == variable))
}

fn nonpositive_numeric(expression: &str) -> bool {
    expression
        .trim_matches(['(', ')', ' '])
        .parse::<f64>()
        .is_ok_and(|value| !value.is_finite() || value <= 0.0)
}

fn positive_numeric(expression: &str) -> bool {
    expression
        .trim_matches(['(', ')', ' '])
        .parse::<f64>()
        .is_ok_and(|value| value.is_finite() && value > 0.0)
}

fn grouped(expression: &str) -> String {
    format!("({})", expression.trim())
}

fn source_expression(request: &ImproperIntegralRequest) -> String {
    format!(
        "Integrate({},{},{})({})",
        request.variable, request.lower, request.upper, request.expression
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EvalResult, Expr, RustEngine};

    struct CountingEngine {
        inner: RustEngine,
        evals: usize,
        expression_evals: usize,
    }

    impl CountingEngine {
        fn spawn() -> Self {
            Self {
                inner: RustEngine::spawn().unwrap(),
                evals: 0,
                expression_evals: 0,
            }
        }
    }

    impl Engine for CountingEngine {
        fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
            self.evals += 1;
            self.inner.eval(command)
        }

        fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
            self.expression_evals += 1;
            self.inner.eval_expr(command)
        }
    }

    fn request(expression: &str) -> ImproperIntegralRequest {
        ImproperIntegralRequest {
            expression: expression.into(),
            variable: "t".into(),
            lower: "0".into(),
            upper: "Infinity".into(),
            singular_points: Vec::new(),
        }
    }

    #[test]
    fn lowers_definition_with_arbitrary_parameter_expression() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = try_lower_improper_integral(&mut engine, &request("t^(1/x-1)*Exp(-t)"))
            .unwrap()
            .unwrap();
        assert_eq!(result.value.replace(' ', ""), "Gamma((1/x))");
        assert!(
            matches!(result.conditions.conditions(), [Condition::RealPartPositive { expression }] if expression.replace(' ', "") == "1/x")
        );
    }

    #[test]
    fn accepts_reordering_divided_powers_constants_and_scale() {
        let mut engine = RustEngine::spawn().unwrap();
        let reordered = try_lower_improper_integral(&mut engine, &request("Exp(-t)*t^(a-1)*3"))
            .unwrap()
            .unwrap();
        assert!(reordered.target.contains("Gamma"));
        assert!(reordered.target.contains('3'));
        let divided = try_lower_improper_integral(&mut engine, &request("t^a/t*Exp(-t)"))
            .unwrap()
            .unwrap();
        assert!(divided.target.contains("Gamma((a))"));
        let scaled = try_lower_improper_integral(&mut engine, &request("t^(a-1)*Exp(-(b*t))"))
            .unwrap()
            .unwrap();
        assert!(scaled.conditions.conditions().iter().any(
            |condition| matches!(condition, Condition::Positive { expression } if expression == "b")
        ));
    }

    #[test]
    fn bounded_misses_fall_back() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(try_lower_improper_integral(&mut engine, &request("Sin(t)"))
            .unwrap()
            .is_none());
        let mut wrong_bounds = request("t^(a-1)*Exp(-t)");
        wrong_bounds.lower = "1".into();
        assert!(try_lower_improper_integral(&mut engine, &wrong_bounds)
            .unwrap()
            .is_none());
        assert!(
            try_lower_improper_integral(&mut engine, &request("t^(-2)*Exp(-t)"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn registry_is_narrowly_keyed() {
        assert_eq!(INTRINSIC_SIGNATURES.len(), 1);
        assert_eq!(
            INTRINSIC_SIGNATURES[0].feature,
            SourceFeature::PositiveHalfLineExponentialKernel
        );
    }

    #[test]
    fn fast_paths_bound_engine_work_and_teaching_metadata() {
        let mut engine = CountingEngine::spawn();
        let miss = try_lower_improper_integral(&mut engine, &request("Sin(t)")).unwrap();
        assert!(miss.is_none());
        assert_eq!((engine.evals, engine.expression_evals), (0, 0));

        let lowered = try_lower_improper_integral(&mut engine, &request("t^(a-1)*Exp(-t)"))
            .unwrap()
            .unwrap();
        assert_eq!((engine.evals, engine.expression_evals), (1, 0));
        assert!(lowered.certificate.is_none());

        let evidenced =
            try_lower_improper_integral_with_certificate(&mut engine, &request("t^(a-1)*Exp(-t)"))
                .unwrap()
                .unwrap();
        assert!(evidenced.certificate.is_some());
        assert_eq!((engine.evals, engine.expression_evals), (2, 0));
    }
}
