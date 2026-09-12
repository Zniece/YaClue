//! Bounded lowering from defined objects to native symbolic intrinsics.

use serde::Serialize;

use crate::binding;
use crate::engine::{Engine, EngineError};
use crate::improper_integrals::ImproperIntegralRequest;
use crate::input::{root_call, strip_tex_delimiters};
use crate::objects::DefinedObjectKind;
use crate::protocol::{Condition, ConditionSet};
use crate::steps::{Step, StepImportance, StepVerbosity};

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
    /// Omitted on the no-steps fast path to keep teaching history out of the
    /// current-value metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<LoweringCertificate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
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
    verbosity: Option<StepVerbosity>,
) -> Result<Option<IntrinsicLoweringResult>, EngineError> {
    let Some(signature) = select_signature(request) else {
        return Ok(None);
    };
    debug_assert_eq!(signature.intrinsic, IntrinsicKind::Gamma);
    let Some(matched) = match_gamma_kernel(request)? else {
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
    let certificate = verbosity.map(|_| LoweringCertificate {
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
    let steps = verbosity
        .map(|_| {
            vec![Step {
                kind: crate::steps::StepKind::EquivalentTransformation,
                before_expr: None,
                before_tex: None,
                rule: "recognize-gamma-integral".into(),
                expr: value.clone(),
                why: "识别 Euler 型积分核，在成立条件下使用原生 Gamma 对象。".into(),
                tex: tex.clone(),
                importance: StepImportance::Key,
            }]
        })
        .unwrap_or_default();
    Ok(Some(IntrinsicLoweringResult {
        intrinsic: IntrinsicKind::Gamma,
        source: source_expression(request),
        target,
        value,
        tex,
        conditions,
        certificate,
        steps,
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

fn match_gamma_kernel(
    request: &ImproperIntegralRequest,
) -> Result<Option<GammaMatch>, EngineError> {
    let (mut factors, denominator_variables) =
        multiplicative_factors(&request.expression, &request.variable)?;
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
        } else if free_of(&factor, &request.variable)? {
            constants.push(factor);
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
    expression: &str,
    variable: &str,
) -> Result<(Vec<String>, usize), EngineError> {
    let Some(call) = root_call(expression, "intrinsic 候选积分核")? else {
        return Ok((vec![expression.into()], 0));
    };
    match (call.head.as_str(), call.arguments.as_slice()) {
        ("*", [left, right]) => {
            let (mut left_factors, left_divisors) = multiplicative_factors(left, variable)?;
            let (right_factors, right_divisors) = multiplicative_factors(right, variable)?;
            left_factors.extend(right_factors);
            Ok((left_factors, left_divisors + right_divisors))
        }
        ("/", [numerator, denominator]) if denominator.trim() == variable => {
            let (factors, divisors) = multiplicative_factors(numerator, variable)?;
            Ok((factors, divisors + 1))
        }
        ("/", [numerator, denominator]) if free_of(denominator, variable)? => {
            let (mut factors, divisors) = multiplicative_factors(numerator, variable)?;
            factors.push(format!("1/({denominator})"));
            Ok((factors, divisors))
        }
        _ => Ok((vec![expression.into()], 0)),
    }
}

fn exponential_scale(factor: &str, variable: &str) -> Result<Option<String>, EngineError> {
    let Some(call) = root_call(factor, "指数核")? else {
        return Ok(None);
    };
    let ("Exp", [argument]) = (call.head.as_str(), call.arguments.as_slice()) else {
        return Ok(None);
    };
    let Some(negative) = root_call(argument, "指数核")? else {
        return Ok(None);
    };
    let ("-", [kernel]) = (negative.head.as_str(), negative.arguments.as_slice()) else {
        return Ok(None);
    };
    if kernel.trim() == variable {
        return Ok(Some("1".into()));
    }
    let Some(product) = root_call(kernel, "指数核")? else {
        return Ok(None);
    };
    let ("*", [left, right]) = (product.head.as_str(), product.arguments.as_slice()) else {
        return Ok(None);
    };
    if left.trim() == variable && free_of(right, variable)? {
        Ok(Some(right.clone()))
    } else if right.trim() == variable && free_of(left, variable)? {
        Ok(Some(left.clone()))
    } else {
        Ok(None)
    }
}

fn variable_power(factor: &str, variable: &str) -> Result<Option<String>, EngineError> {
    if factor.trim() == variable {
        return Ok(Some("1".into()));
    }
    let Some(call) = root_call(factor, "intrinsic 幂核")? else {
        return Ok(None);
    };
    match (call.head.as_str(), call.arguments.as_slice()) {
        ("^", [base, exponent]) if base.trim() == variable => Ok(Some(exponent.clone())),
        _ => Ok(None),
    }
}

fn gamma_argument(exponent: &str) -> Result<String, EngineError> {
    if let Ok(value) = exponent.trim_matches(['(', ')', ' ']).parse::<f64>() {
        if value.is_finite() {
            return Ok((value + 1.0).to_string());
        }
    }
    if let Some(call) = root_call(exponent, "Gamma 幂指数")? {
        if let ("-", [argument, one]) = (call.head.as_str(), call.arguments.as_slice()) {
            if one.trim() == "1" {
                return Ok(argument.clone());
            }
        }
    }
    Ok(format!("({exponent})+1"))
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
        let result = try_lower_improper_integral(
            &mut engine,
            &request("t^(1/x-1)*Exp(-t)"),
            Some(StepVerbosity::Standard),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.value.replace(' ', ""), "Gamma((1/x))");
        assert!(
            matches!(result.conditions.conditions(), [Condition::RealPartPositive { expression }] if expression.replace(' ', "") == "1/x")
        );
        assert!(!result.steps.is_empty());
    }

    #[test]
    fn accepts_reordering_divided_powers_constants_and_scale() {
        let mut engine = RustEngine::spawn().unwrap();
        let reordered =
            try_lower_improper_integral(&mut engine, &request("Exp(-t)*t^(a-1)*3"), None)
                .unwrap()
                .unwrap();
        assert!(reordered.target.contains("Gamma"));
        assert!(reordered.target.contains('3'));
        let divided = try_lower_improper_integral(&mut engine, &request("t^a/t*Exp(-t)"), None)
            .unwrap()
            .unwrap();
        assert!(divided.target.contains("Gamma((a))"));
        let scaled =
            try_lower_improper_integral(&mut engine, &request("t^(a-1)*Exp(-(b*t))"), None)
                .unwrap()
                .unwrap();
        assert!(scaled.conditions.conditions().iter().any(
            |condition| matches!(condition, Condition::Positive { expression } if expression == "b")
        ));
    }

    #[test]
    fn bounded_misses_fall_back() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(
            try_lower_improper_integral(&mut engine, &request("Sin(t)"), None)
                .unwrap()
                .is_none()
        );
        let mut wrong_bounds = request("t^(a-1)*Exp(-t)");
        wrong_bounds.lower = "1".into();
        assert!(
            try_lower_improper_integral(&mut engine, &wrong_bounds, None)
                .unwrap()
                .is_none()
        );
        assert!(
            try_lower_improper_integral(&mut engine, &request("t^(-2)*Exp(-t)"), None)
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
        let miss = try_lower_improper_integral(&mut engine, &request("Sin(t)"), None).unwrap();
        assert!(miss.is_none());
        assert_eq!((engine.evals, engine.expression_evals), (0, 0));

        let lowered = try_lower_improper_integral(&mut engine, &request("t^(a-1)*Exp(-t)"), None)
            .unwrap()
            .unwrap();
        assert_eq!((engine.evals, engine.expression_evals), (1, 0));
        assert!(lowered.steps.is_empty());
        assert!(lowered.certificate.is_none());

        let stepped = try_lower_improper_integral(
            &mut engine,
            &request("t^(a-1)*Exp(-t)"),
            Some(StepVerbosity::Detailed),
        )
        .unwrap()
        .unwrap();
        assert_eq!((engine.evals, engine.expression_evals), (2, 0));
        assert_eq!(stepped.steps.len(), 1);
        assert_eq!(stepped.certificate.unwrap().bindings.len(), 4);
    }
}
