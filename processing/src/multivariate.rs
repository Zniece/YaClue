//! Structured processing API for multivariate differential operations.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{
    analyze_expression, fresh_internal_symbols, strip_tex_delimiters, validate_expression,
    validate_symbol,
};
use serde::Serialize;
use std::collections::BTreeSet;
use yacas_rs::value::{spine_refs, ObjectKind};

use crate::protocol::{ConditionSet, OutcomeReason, ResultMetadata};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    CapabilitySet, Certificate, Computation, ComputationOutput, NormalizationLevel,
    NormalizationMetadata, NormalizationMode, ObjectDelta, OperatorId, RuleEvent, RuleImportance,
    RulePayload, RulePresentation, RuleTrace, SemanticInterpretation, SemanticOperation,
    SemanticState,
};

pub const MAX_MULTIVARIATE_DIMENSION: usize = 16;
pub const MAX_PARTIAL_ORDER: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MultivariateOperation {
    PartialDerivative,
    Gradient,
    Jacobian,
    Hessian,
    Divergence,
    Curl,
    DirectionalDerivative,
}

#[derive(Debug, Clone)]
struct MultivariateRequest {
    pub operation: MultivariateOperation,
    pub expressions: Vec<String>,
    pub variables: Vec<String>,
    pub direction: Vec<String>,
    /// Optional coordinates at which the completed derivative is evaluated.
    pub point: Vec<String>,
    pub order: u32,
    pub normalize_direction: bool,
}

#[derive(Debug, Clone)]
pub struct MultivariateObjectRequest {
    pub operation: MultivariateOperation,
    pub variables: Vec<String>,
    pub direction: Vec<String>,
    pub point: Vec<String>,
    pub order: u32,
    pub normalize_direction: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MultivariateDifferentialOperation;

impl SemanticOperation<MultivariateObjectRequest> for MultivariateDifferentialOperation {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &MultivariateObjectRequest,
    ) -> Result<Computation, EngineError> {
        if !input
            .semantics
            .capabilities
            .contains(crate::semantic_core::ObjectCapability::AnalyzeMultivariate)
        {
            return Err(EngineError::InvalidInput(
                "该数学对象不具备多元微分能力".into(),
            ));
        }
        let legacy = MultivariateRequest {
            operation: request.operation,
            expressions: object_components(input),
            variables: request.variables.clone(),
            direction: request.direction.clone(),
            point: request.point.clone(),
            order: request.order,
            normalize_direction: request.normalize_direction,
        };
        let result = compute(engine, &legacy)?;
        let source = result.output.clone();
        let parsed = crate::semantic_core::parse_engine_expression(&source)?;
        let metadata = if result.unresolved {
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::AlgorithmUncovered)
        } else {
            ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
        };
        let mut output = input.clone();
        output.apply(ObjectDelta {
            expression: Some(parsed.raw_expression()),
            semantics: Some(SemanticState {
                kind: if result.unresolved {
                    ValueKind::Unevaluated
                } else {
                    crate::semantic::analyze_input(&source, "多元微分结果")?
                        .semantic
                        .kind
                },
                interpretation: if result.unresolved {
                    SemanticInterpretation::HeldApplication {
                        operator: format!("{:?}", request.operation),
                    }
                } else {
                    SemanticInterpretation::PlainExpression
                },
                metadata,
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            }),
            overlay: None,
            normalization: (!result.unresolved).then_some(NormalizationMetadata {
                level: NormalizationLevel::Domain,
                assumptions: Vec::new(),
                mode: NormalizationMode::Operation(OperatorId::MultivariateDifferential),
            }),
        });
        let event = RuleEvent {
            rule: "multivariate-differential".into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: request
                .variables
                .iter()
                .cloned()
                .map(|variable| ("variable".into(), variable))
                .collect(),
            conditions: Vec::new(),
            payload: RulePayload::Structural,
            importance: RuleImportance::Key,
            transformation: None,
            presentation: Some(RulePresentation {
                expression: source,
                explanation: format!("执行 {:?} 多元微分运算。", request.operation),
                tex_override: Some(result.tex),
            }),
        };
        Ok(Computation {
            output: if result.unresolved {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: vec![Certificate {
                kind: "multivariate_shape".into(),
                payload: serde_json::to_string(&result.shape)
                    .map_err(|error| EngineError::Parse(error.to_string()))?,
            }],
            effects: Vec::new(),
        })
    }
}

fn object_components(input: &crate::semantic_core::MathematicalObject) -> Vec<String> {
    let expression = input.raw_expression();
    let ObjectKind::Sublist(first) = &expression.kind else {
        return vec![input.print_source()];
    };
    let nodes = spine_refs(first).collect::<Vec<_>>();
    if nodes
        .first()
        .and_then(|node| node.atom_string())
        .is_none_or(|head| head.as_ref() != "List")
    {
        return vec![input.print_source()];
    }
    crate::input::with_parse_env(|env| {
        nodes
            .iter()
            .skip(1)
            .map(|node| yacas_rs::printer::infix_print(env, node))
            .collect()
    })
}

#[derive(Debug, Clone, Serialize)]
struct MultivariateResult {
    pub operation: MultivariateOperation,
    pub output: String,
    pub tex: String,
    /// Scalar results use an empty shape, vectors use `[n]`, and matrices use
    /// `[rows, columns]`.
    pub shape: Vec<usize>,
    pub evaluated_at: Vec<String>,
    pub unresolved: bool,
}

fn compute(
    engine: &mut dyn Engine,
    request: &MultivariateRequest,
) -> Result<MultivariateResult, EngineError> {
    validate_request(request)?;
    let command = command(request);
    let result = engine.eval(&command)?;
    Ok(MultivariateResult {
        operation: request.operation,
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        shape: result_shape(request),
        evaluated_at: request.point.clone(),
        unresolved: contains_derivative_head(&result.expr),
    })
}

fn validate_request(request: &MultivariateRequest) -> Result<(), EngineError> {
    if request.variables.is_empty() || request.variables.len() > MAX_MULTIVARIATE_DIMENSION {
        return Err(EngineError::InvalidInput(format!(
            "变量数量必须在 1..={MAX_MULTIVARIATE_DIMENSION} 之间"
        )));
    }
    let mut unique = BTreeSet::new();
    for variable in &request.variables {
        validate_symbol(variable, "微分变量")?;
        if !unique.insert(variable) {
            return Err(EngineError::InvalidInput(format!(
                "微分变量不能重复: {variable}"
            )));
        }
    }
    for expression in &request.expressions {
        validate_expression(expression, "多元表达式")?;
    }
    if request.expressions.len() > MAX_MULTIVARIATE_DIMENSION {
        return Err(EngineError::InvalidInput(format!(
            "表达式数量不能超过 {MAX_MULTIVARIATE_DIMENSION}"
        )));
    }
    for component in &request.direction {
        validate_expression(component, "方向分量")?;
    }
    if !request.point.is_empty() && request.point.len() != request.variables.len() {
        return Err(EngineError::InvalidInput(
            "求值点维度必须与变量数量一致".into(),
        ));
    }
    for coordinate in &request.point {
        let analysis = analyze_expression(coordinate, "求值点坐标")?;
        if analysis
            .symbols
            .iter()
            .any(|symbol| request.variables.contains(symbol))
        {
            return Err(EngineError::InvalidInput(
                "求值点坐标不能依赖正在替换的变量".into(),
            ));
        }
    }

    let valid_expression_count = match request.operation {
        MultivariateOperation::PartialDerivative
        | MultivariateOperation::Gradient
        | MultivariateOperation::Hessian
        | MultivariateOperation::DirectionalDerivative => request.expressions.len() == 1,
        MultivariateOperation::Jacobian => !request.expressions.is_empty(),
        MultivariateOperation::Divergence | MultivariateOperation::Curl => {
            request.expressions.len() == request.variables.len()
        }
    };
    if !valid_expression_count {
        return Err(EngineError::InvalidInput(format!(
            "{:?} 的表达式数量与运算维度不匹配",
            request.operation
        )));
    }
    if request.operation == MultivariateOperation::Curl && request.variables.len() != 3 {
        return Err(EngineError::InvalidInput("Curl 只接受三维向量场".into()));
    }
    if request.operation == MultivariateOperation::PartialDerivative {
        if request.variables.len() != 1 {
            return Err(EngineError::InvalidInput(
                "偏导请求必须指定一个微分变量".into(),
            ));
        }
        if request.order == 0 || request.order > MAX_PARTIAL_ORDER {
            return Err(EngineError::InvalidInput(format!(
                "偏导阶数必须在 1..={MAX_PARTIAL_ORDER} 之间"
            )));
        }
    }
    if request.operation == MultivariateOperation::DirectionalDerivative {
        if request.direction.len() != request.variables.len() {
            return Err(EngineError::InvalidInput(
                "方向向量维度必须与变量数量一致".into(),
            ));
        }
    } else if !request.direction.is_empty() {
        return Err(EngineError::InvalidInput("只有方向导数接受方向向量".into()));
    }
    Ok(())
}

fn command(request: &MultivariateRequest) -> String {
    let variables = list(&request.variables);
    let expressions = list(&request.expressions);
    let operation = match request.operation {
        MultivariateOperation::PartialDerivative => format!(
            "Deriv({}, {})({})",
            request.variables[0], request.order, request.expressions[0]
        ),
        MultivariateOperation::Gradient => gradient(&request.expressions[0], &request.variables),
        MultivariateOperation::Jacobian => format!("JacobianMatrix({expressions},{variables})"),
        MultivariateOperation::Hessian => {
            format!("HessianMatrix({},{variables})", request.expressions[0])
        }
        MultivariateOperation::Divergence => format!("Diverge({expressions},{variables})"),
        MultivariateOperation::Curl => format!("Curl({expressions},{variables})"),
        MultivariateOperation::DirectionalDerivative => {
            let products = request
                .variables
                .iter()
                .zip(&request.direction)
                .map(|(variable, direction)| {
                    format!(
                        "(Deriv({variable})({}))*({direction})",
                        request.expressions[0]
                    )
                })
                .collect::<Vec<_>>()
                .join("+");
            if request.normalize_direction {
                let inputs = request
                    .expressions
                    .iter()
                    .chain(&request.variables)
                    .chain(&request.direction)
                    .chain(&request.point)
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                let [norm] =
                    fresh_internal_symbols("DirectionalDerivative", &inputs, ["NormSquared"]);
                let norm_squared = request
                    .direction
                    .iter()
                    .map(|component| format!("({component})^2"))
                    .collect::<Vec<_>>()
                    .join("+");
                format!(
                    "[Local({norm}); {norm}:=Simplify({norm_squared}); \
                     Check(Not(IsZero({norm})),\"direction vector must be nonzero\"); \
                     Simplify(({products})/Sqrt({norm}));]"
                )
            } else {
                format!("Simplify({products})")
            }
        }
    };
    let evaluated = request.variables.iter().zip(&request.point).fold(
        operation,
        |expression, (variable, coordinate)| {
            format!("Subst({variable},{coordinate})({expression})")
        },
    );
    if request.point.is_empty() {
        evaluated
    } else {
        format!("Simplify({evaluated})")
    }
}

fn gradient(expression: &str, variables: &[String]) -> String {
    let components = variables
        .iter()
        .map(|variable| format!("Deriv({variable})({expression})"))
        .collect::<Vec<_>>();
    list(&components)
}

fn list(values: &[String]) -> String {
    format!("{{{}}}", values.join(","))
}

fn result_shape(request: &MultivariateRequest) -> Vec<usize> {
    match request.operation {
        MultivariateOperation::Gradient | MultivariateOperation::Curl => {
            vec![request.variables.len()]
        }
        MultivariateOperation::Jacobian => {
            vec![request.expressions.len(), request.variables.len()]
        }
        MultivariateOperation::Hessian => {
            vec![request.variables.len(), request.variables.len()]
        }
        MultivariateOperation::PartialDerivative
        | MultivariateOperation::Divergence
        | MultivariateOperation::DirectionalDerivative => vec![],
    }
}

fn contains_derivative_head(expression: &Expr) -> bool {
    match expression {
        Expr::Call { head, args } => {
            matches!(
                head.as_str(),
                "Deriv" | "D" | "JacobianMatrix" | "HessianMatrix"
            ) || args.iter().any(contains_derivative_head)
        }
        Expr::Number(_) | Expr::Symbol(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    fn request(
        operation: MultivariateOperation,
        expressions: &[&str],
        variables: &[&str],
    ) -> MultivariateRequest {
        MultivariateRequest {
            operation,
            expressions: expressions.iter().map(|value| (*value).into()).collect(),
            variables: variables.iter().map(|value| (*value).into()).collect(),
            direction: vec![],
            point: vec![],
            order: 1,
            normalize_direction: false,
        }
    }

    #[test]
    fn computes_scalar_vector_and_matrix_differential_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        let cases = [
            (
                request(MultivariateOperation::PartialDerivative, &["x^2*y"], &["x"]),
                "(2 * (x * y))",
                vec![],
            ),
            (
                request(MultivariateOperation::Gradient, &["x^2+y^2"], &["x", "y"]),
                "List((2 * x),(2 * y))",
                vec![2],
            ),
            (
                request(
                    MultivariateOperation::Jacobian,
                    &["x+y", "x*y", "x^2"],
                    &["x", "y"],
                ),
                "List(List(1,1),List(y,x),List((2 * x),0))",
                vec![3, 2],
            ),
            (
                request(MultivariateOperation::Hessian, &["x^2*y+y^3"], &["x", "y"]),
                "List(List((2 * y),(2 * x)),List((2 * x),(6 * y)))",
                vec![2, 2],
            ),
            (
                request(
                    MultivariateOperation::Divergence,
                    &["x^2", "y^2", "z^2"],
                    &["x", "y", "z"],
                ),
                "(((2 * x) + (2 * y)) + (2 * z))",
                vec![],
            ),
            (
                request(
                    MultivariateOperation::Curl,
                    &["y*z", "x*z", "x*y"],
                    &["x", "y", "z"],
                ),
                "List(0,0,0)",
                vec![3],
            ),
        ];
        for (request, expected, shape) in cases {
            let result = compute(&mut engine, &request).unwrap();
            assert_eq!(result.output, expected);
            assert_eq!(result.shape, shape);
            assert!(!result.tex.is_empty());
            assert!(!result.unresolved);
        }
    }

    #[test]
    fn computes_normalized_and_raw_directional_derivatives() {
        let mut engine = RustEngine::spawn().unwrap();
        let mut request = request(
            MultivariateOperation::DirectionalDerivative,
            &["x^2+y^2"],
            &["x", "y"],
        );
        request.direction = vec!["3".into(), "4".into()];
        request.normalize_direction = true;
        assert_eq!(
            compute(&mut engine, &request).unwrap().output,
            "((2 * ((3 * x) + (4 * y))) / 5)"
        );

        request.normalize_direction = false;
        assert_eq!(
            compute(&mut engine, &request).unwrap().output,
            "(2 * ((3 * x) + (4 * y)))"
        );
    }

    #[test]
    fn normalized_direction_preserves_former_norm_temporary() {
        let mut engine = RustEngine::spawn().unwrap();
        let mut request = request(
            MultivariateOperation::DirectionalDerivative,
            &["x"],
            &["x", "y"],
        );
        request.direction = vec!["n".into(), "0".into()];
        request.normalize_direction = true;
        let result = compute(&mut engine, &request).unwrap();
        assert!(result.output.contains('n'));
        assert!(!result
            .output
            .contains("YaClueDirectionalDerivativeInternal"));
    }

    #[test]
    fn evaluates_the_completed_derivative_at_a_point() {
        let mut engine = RustEngine::spawn().unwrap();
        let mut request = request(MultivariateOperation::Gradient, &["x^2+y^2"], &["x", "y"]);
        request.point = vec!["1".into(), "-2".into()];
        let result = compute(&mut engine, &request).unwrap();
        assert_eq!(result.output, "List(2,-4)");
        assert_eq!(result.evaluated_at, request.point);
    }

    #[test]
    fn rejects_invalid_dimensions_symbols_and_direction_vectors() {
        let mut engine = RustEngine::spawn().unwrap();
        let mut curl = request(MultivariateOperation::Curl, &["y", "x"], &["x", "y"]);
        assert!(compute(&mut engine, &curl).is_err());

        curl.variables = vec!["x);Echo(1);(".into(), "y".into()];
        assert!(compute(&mut engine, &curl).is_err());

        let mut directional = request(
            MultivariateOperation::DirectionalDerivative,
            &["x+y"],
            &["x", "y"],
        );
        directional.direction = vec!["0".into(), "0".into()];
        directional.normalize_direction = true;
        assert!(compute(&mut engine, &directional).is_err());

        directional.direction = vec!["1".into(), "0".into()];
        directional.point = vec!["y".into(), "0".into()];
        assert!(compute(&mut engine, &directional).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }
}
