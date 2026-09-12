//! Typed structural composition for scalar/symbolic expressions.

use crate::engine::{Engine, EngineError};
use crate::protocol::{
    Condition, ConditionSet, Conditionality, OutcomeReason, ResolutionState, ResultMetadata,
};
use crate::semantic::{Exactness, ValueKind};
use crate::semantic_core::{
    object_from_source, BinarySemanticOperation, CapabilitySet, Computation, ComputationOutput,
    NormalizationLevel, NormalizationMetadata, NormalizationMode, ObjectCapability, ObjectDelta,
    ObjectId, OperatorId, RuleEvent, RuleImportance, RulePayload, RulePresentation, RuleTrace,
    SemanticInterpretation, SemanticOperation, SemanticState, UnarySemanticOperation,
};

/// Recursively execute a structural elaboration tree without reparsing its
/// children. Registered domain applications remain typed Held operands until
/// their own executor has lowered them.
pub fn can_execute_elaborated_tree(expression: &crate::elaboration::ElaboratedObject) -> bool {
    match &expression.form {
        crate::elaboration::MathematicalForm::Structural { .. } => {
            !expression
                .children
                .iter()
                .any(|child| matches!(child.form, crate::elaboration::MathematicalForm::Collection))
                && expression.children.iter().all(can_execute_elaborated_tree)
        }
        crate::elaboration::MathematicalForm::Application { head }
            if crate::semantic_core::is_object_native_operator(head) =>
        {
            use crate::semantic_core::ObjectNativeRoute;
            match crate::semantic_core::object_native_route(head).unwrap() {
                ObjectNativeRoute::Calculus => {
                    let Some(operand_index) = crate::semantic_core::signature_requirements(
                        head,
                        expression.children.len(),
                    )
                    .and_then(|requirements| {
                        requirements
                            .iter()
                            .position(|item| *item == crate::semantic_core::Requirement::Operand)
                    }) else {
                        return false;
                    };
                    can_execute_elaborated_tree(&expression.children[operand_index])
                }
                ObjectNativeRoute::AlgebraTransform => expression
                    .children
                    .first()
                    .is_some_and(can_execute_elaborated_tree),
                ObjectNativeRoute::MatrixSolve => {
                    expression.children.len() == 2
                        && expression.children.iter().all(can_execute_elaborated_tree)
                }
                ObjectNativeRoute::Substitute => {
                    expression.children.len() == 3
                        && can_execute_elaborated_tree(&expression.children[1])
                        && can_execute_elaborated_tree(&expression.children[2])
                }
                ObjectNativeRoute::Approximate => {
                    matches!(expression.children.len(), 1 | 2)
                        && can_execute_elaborated_tree(&expression.children[0])
                }
                ObjectNativeRoute::Taylor => {
                    let Some(operand_index) = crate::semantic_core::signature_requirements(
                        head,
                        expression.children.len(),
                    )
                    .and_then(|requirements| {
                        requirements
                            .iter()
                            .position(|item| *item == crate::semantic_core::Requirement::Operand)
                    }) else {
                        return false;
                    };
                    can_execute_elaborated_tree(&expression.children[operand_index])
                }
                ObjectNativeRoute::EquationSolve => {
                    expression.children.len() == 2
                        && can_execute_elaborated_tree(&expression.children[0])
                }
                ObjectNativeRoute::OdeSolve
                | ObjectNativeRoute::MatrixUnary
                | ObjectNativeRoute::FactorProjection => {
                    expression.children.len() == 1
                        && can_execute_elaborated_tree(&expression.children[0])
                }
                ObjectNativeRoute::Series => {
                    expression.children.len() == 4
                        && can_execute_elaborated_tree(&expression.children[3])
                }
            }
        }
        crate::elaboration::MathematicalForm::Application { head } => {
            !crate::semantic_core::is_known_operator(head)
                && expression.children.iter().all(can_execute_elaborated_tree)
        }
        crate::elaboration::MathematicalForm::EffectApplication { .. } => true,
        crate::elaboration::MathematicalForm::OpaqueEngineValue { .. } => false,
        crate::elaboration::MathematicalForm::Relation { .. }
        | crate::elaboration::MathematicalForm::Collection => {
            expression.children.iter().all(can_execute_elaborated_tree)
        }
        _ => true,
    }
}

pub fn has_object_native_descendant(expression: &crate::elaboration::ElaboratedObject) -> bool {
    expression.children.iter().any(|child| {
        matches!(&child.form,
            crate::elaboration::MathematicalForm::Application { head }
                if crate::semantic_core::is_object_native_operator(head))
            || has_object_native_descendant(child)
    })
}

pub fn has_effect_descendant(expression: &crate::elaboration::ElaboratedObject) -> bool {
    expression.children.iter().any(|child| {
        matches!(
            child.form,
            crate::elaboration::MathematicalForm::EffectApplication { .. }
        ) || has_effect_descendant(child)
    })
}

pub fn contains_operator(
    expression: &crate::elaboration::ElaboratedObject,
    expected: OperatorId,
) -> bool {
    matches!(&expression.form,
        crate::elaboration::MathematicalForm::Application { head }
            if crate::semantic_core::operator_descriptor(head)
                .is_some_and(|descriptor| descriptor.id == expected))
        || expression
            .children
            .iter()
            .any(|child| contains_operator(child, expected))
}

pub fn execute_elaborated_structure(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    if let crate::elaboration::MathematicalForm::Application { head } = &expression.form {
        if let Some(lowered) = crate::lowering::try_lower_application(
            engine,
            &expression.object,
            crate::semantic_core::TraceMode::Detailed,
        )? {
            return Ok(lowered);
        }
        use crate::semantic_core::ObjectNativeRoute;
        if let Some(route) = crate::semantic_core::object_native_route(head) {
            return match route {
                ObjectNativeRoute::Calculus => {
                    execute_calculus_application(engine, expression, head)
                }
                ObjectNativeRoute::AlgebraTransform => {
                    execute_transform_application(engine, expression, head)
                }
                ObjectNativeRoute::Substitute => {
                    execute_substitution_application(engine, expression)
                }
                ObjectNativeRoute::Approximate => {
                    execute_numeric_application(engine, expression, head)
                }
                ObjectNativeRoute::Taylor => execute_taylor_application(engine, expression),
                ObjectNativeRoute::EquationSolve => execute_solve_application(engine, expression),
                ObjectNativeRoute::OdeSolve => execute_ode_solve_application(engine, expression),
                ObjectNativeRoute::MatrixUnary => {
                    execute_matrix_unary_application(engine, expression, head)
                }
                ObjectNativeRoute::FactorProjection => {
                    execute_factor_projection_application(engine, expression)
                }
                ObjectNativeRoute::MatrixSolve => {
                    execute_matrix_solve_application(engine, expression)
                }
                ObjectNativeRoute::Series => execute_sum_application(engine, expression),
            };
        }
        if !crate::semantic_core::is_known_operator(head) {
            return execute_function_application(engine, expression, head);
        }
    }
    if matches!(
        expression.form,
        crate::elaboration::MathematicalForm::Relation { .. }
            | crate::elaboration::MathematicalForm::Collection
    ) {
        return execute_container(expression, engine);
    }
    if matches!(
        expression.form,
        crate::elaboration::MathematicalForm::EffectApplication { .. }
    ) {
        return Ok(Computation {
            output: ComputationOutput::EffectsOnly,
            trace: None,
            certificates: Vec::new(),
            effects: Vec::new(),
        });
    }
    let crate::elaboration::MathematicalForm::Structural { operator } = &expression.form else {
        return Ok(Computation {
            output: if expression.object.semantics.metadata.resolution
                == ResolutionState::Unresolved
            {
                ComputationOutput::Held(expression.object.clone())
            } else {
                ComputationOutput::Value(expression.object.clone())
            },
            trace: None,
            certificates: Vec::new(),
            effects: Vec::new(),
        });
    };
    let operation = match (operator.as_str(), expression.children.len()) {
        ("-", 1) => ArithmeticOperation::Negate,
        ("+", 2) => ArithmeticOperation::Add,
        ("-", 2) => ArithmeticOperation::Subtract,
        ("*", 2) => ArithmeticOperation::Multiply,
        ("/", 2) => ArithmeticOperation::Divide,
        ("^", 2) => ArithmeticOperation::Power,
        _ => {
            return Err(EngineError::InvalidInput(format!(
                "结构运算 {operator} 不支持 {} 个操作数",
                expression.children.len()
            )))
        }
    };
    let mut child_computations = expression
        .children
        .iter()
        .map(|child| execute_elaborated_structure(engine, child))
        .collect::<Result<Vec<_>, _>>()?;
    let mut child_traces = Vec::new();
    for child in &mut child_computations {
        if let Some(trace) = child.trace.take() {
            child_traces.extend(trace.events);
        }
    }
    if child_computations
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::EffectsOnly))
    {
        return Err(EngineError::InvalidInput(
            "带副作用的动作不能作为数学结构运算的操作数".into(),
        ));
    }
    let request = ArithmeticRequest {
        output_id: expression.object.id,
        operation,
    };
    let left_object = child_computations[0]
        .subject()
        .expect("mathematical child has an object");
    let right_object = child_computations.get(1).and_then(Computation::subject);
    let left_matrix = matches!(
        left_object.semantics.interpretation,
        SemanticInterpretation::Matrix { .. }
    );
    let right_matrix = right_object.is_some_and(|right| {
        matches!(
            right.semantics.interpretation,
            SemanticInterpretation::Matrix { .. }
        )
    });
    let matrix_binary = matches!(
        operation,
        ArithmeticOperation::Add | ArithmeticOperation::Subtract | ArithmeticOperation::Multiply
    ) && (left_matrix || right_matrix);
    let mut computation = if matrix_binary {
        let matrix_operation = match operation {
            ArithmeticOperation::Add => crate::linear_algebra::MatrixOperation::Add,
            ArithmeticOperation::Subtract => crate::linear_algebra::MatrixOperation::Subtract,
            ArithmeticOperation::Multiply if left_matrix && right_matrix => {
                crate::linear_algebra::MatrixOperation::Multiply
            }
            ArithmeticOperation::Multiply => crate::linear_algebra::MatrixOperation::Scale,
            _ => unreachable!(),
        };
        let (matrix, other) = if left_matrix {
            (
                left_object,
                right_object.expect("matrix binary has right operand"),
            )
        } else {
            (
                right_object.expect("matrix binary has matrix operand"),
                left_object,
            )
        };
        BinarySemanticOperation::compute(
            &crate::linear_algebra::BinaryMatrixOperation,
            engine,
            matrix,
            other,
            &crate::linear_algebra::BinaryMatrixRequest {
                operation: matrix_operation,
                output_id: expression.object.id,
            },
        )?
    } else if operation == ArithmeticOperation::Negate {
        UnarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            engine,
            child_computations[0]
                .subject()
                .expect("mathematical child has an object"),
            &request,
        )?
    } else {
        BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            engine,
            child_computations[0]
                .subject()
                .expect("mathematical child has an object"),
            child_computations[1]
                .subject()
                .expect("mathematical child has an object"),
            &request,
        )?
    };
    if let Some(trace) = computation.trace.as_mut() {
        child_traces.append(&mut trace.events);
        trace.events = child_traces;
    }
    Ok(computation)
}

fn execute_sum_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [variable, lower, upper, term_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(
            "Sum 需要变量、下限、上限和求和项".into(),
        ));
    };
    let mut term = execute_elaborated_structure(engine, term_node)?;
    if !matches!(term.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, term, "Sum", 3);
    }
    let input = term.value().expect("checked sum term").clone();
    let mut current = crate::series::SumOperation.compute(
        engine,
        &input,
        &crate::series::SumRequest {
            variable: variable.object.print_source(),
            lower: lower.object.print_source(),
            upper: upper.object.print_source(),
        },
    )?;
    merge_prior_computation(&mut current, &mut term);
    Ok(current)
}

fn execute_numeric_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let (operand_node, precision) = match expression.children.as_slice() {
        [operand] => (operand, 10),
        [operand, precision] => (
            operand,
            precision
                .object
                .print_source()
                .parse::<u32>()
                .map_err(|_| EngineError::InvalidInput("数值精度必须是正整数".into()))?,
        ),
        _ => {
            return Err(EngineError::InvalidInput(format!(
                "{head} 需要 expression 和可选 precision"
            )))
        }
    };
    let mut operand = execute_elaborated_structure(engine, operand_node)?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, operand, head, 0);
    }
    let input = operand.value().expect("checked numeric operand").clone();
    if !input
        .semantics
        .capabilities
        .contains(ObjectCapability::NumericEvaluate)
    {
        return retain_pending_application(expression, operand, head, 0);
    }
    let mut current = crate::numeric::NumericEvaluationOperation.compute(
        engine,
        &input,
        &crate::numeric::NumericEvaluationRequest {
            precision_digits: precision,
        },
    )?;
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn execute_taylor_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let (operand_index, variable, point, degree_source) = match expression.children.as_slice() {
        [_operand, point, degree] => (
            0,
            "x".to_string(),
            point.object.print_source(),
            degree.object.print_source(),
        ),
        [variable, point, degree, _operand] => (
            3,
            variable.object.print_source(),
            point.object.print_source(),
            degree.object.print_source(),
        ),
        _ => {
            return Err(EngineError::InvalidInput(
                "Taylor 需要 expression、point、degree，或 variable、point、degree、operand".into(),
            ))
        }
    };
    let degree = degree_source
        .parse::<u32>()
        .map_err(|_| EngineError::InvalidInput("Taylor 阶数必须是非负整数".into()))?;
    let mut operand = execute_elaborated_structure(engine, &expression.children[operand_index])?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, operand, "Taylor", operand_index);
    }
    let input = operand.value().expect("checked Taylor operand").clone();
    let mut current = crate::numeric::TaylorOperation.compute(
        engine,
        &input,
        &crate::numeric::TaylorRequest {
            variable,
            point,
            degree,
        },
    )?;
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn execute_solve_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [equations_node, variables_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(
            "Solve 需要 equations 和 variables".into(),
        ));
    };
    let mut equations = execute_elaborated_structure(engine, equations_node)?;
    if !matches!(equations.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, equations, "Solve", 0);
    }
    let input = equations.value().expect("checked equation input").clone();
    let equation_sources = if matches!(
        equations_node.form,
        crate::elaboration::MathematicalForm::Collection
    ) {
        crate::input::with_parse_env(|env| {
            input
                .view(env)
                .arguments()
                .into_iter()
                .map(|view| view.print_source())
                .collect()
        })
    } else {
        vec![input.print_source()]
    };
    let variables = if matches!(
        variables_node.form,
        crate::elaboration::MathematicalForm::Collection
    ) {
        variables_node
            .children
            .iter()
            .map(|node| node.object.print_source())
            .collect()
    } else {
        vec![variables_node.object.print_source()]
    };
    let mut current = crate::equations::SolveOperation.compute(
        engine,
        &input,
        &crate::equations::SolveRequest {
            equations: equation_sources,
            variables,
        },
    )?;
    merge_prior_computation(&mut current, &mut equations);
    Ok(current)
}

fn execute_ode_solve_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [equation_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput("OdeSolve 需要一个方程".into()));
    };
    let mut equation = execute_elaborated_structure(engine, equation_node)?;
    if !matches!(equation.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, equation, "OdeSolve", 0);
    }
    let input = equation.value().expect("checked ODE equation").clone();
    let mut current = crate::ode::OdeSolveOperation.compute(
        engine,
        &input,
        &crate::ode::OdeSolveRequest {
            independent: "x".into(),
            dependent: "y".into(),
        },
    )?;
    merge_prior_computation(&mut current, &mut equation);
    Ok(current)
}

fn execute_matrix_unary_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let [operand_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(format!("{head} 需要一个矩阵")));
    };
    let mut operand = execute_elaborated_structure(engine, operand_node)?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, operand, head, 0);
    }
    let input = operand.value().expect("checked matrix operand").clone();
    if matches!(
        head,
        "PLDU" | "Cholesky" | "GramSchmidt" | "OrthogonalBasis" | "OrthonormalBasis"
    ) {
        let kind = match head {
            "PLDU" => crate::linear_algebra::MatrixDecompositionKind::Pldu,
            "Cholesky" => crate::linear_algebra::MatrixDecompositionKind::Cholesky,
            "GramSchmidt" | "OrthonormalBasis" => {
                crate::linear_algebra::MatrixDecompositionKind::GramSchmidt { normalized: true }
            }
            "OrthogonalBasis" => {
                crate::linear_algebra::MatrixDecompositionKind::GramSchmidt { normalized: false }
            }
            _ => unreachable!(),
        };
        let mut current =
            crate::linear_algebra::MatrixDecompositionOperation.compute(engine, &input, &kind)?;
        merge_prior_computation(&mut current, &mut operand);
        return Ok(current);
    }
    let operation = match head {
        "Transpose" => crate::linear_algebra::MatrixOperation::Transpose,
        "Determinant" => crate::linear_algebra::MatrixOperation::Determinant,
        "Inverse" => crate::linear_algebra::MatrixOperation::Inverse,
        _ => {
            let kind = match head {
                "Rank" => crate::linear_algebra::MatrixAnalysisKind::Rank,
                "RREF" | "RowReduce" => crate::linear_algebra::MatrixAnalysisKind::Rref,
                "EigenValues" => crate::linear_algebra::MatrixAnalysisKind::Eigenvalues,
                "NullSpace" => crate::linear_algebra::MatrixAnalysisKind::NullSpace,
                "ColumnSpace" => crate::linear_algebra::MatrixAnalysisKind::ColumnSpace,
                "EigenSpaces" => crate::linear_algebra::MatrixAnalysisKind::EigenSpaces,
                _ => unreachable!(),
            };
            let mut current =
                crate::linear_algebra::MatrixAnalysisOperation.compute(engine, &input, &kind)?;
            merge_prior_computation(&mut current, &mut operand);
            return Ok(current);
        }
    };
    let mut current = crate::linear_algebra::UnaryMatrixOperation.compute(
        engine,
        &input,
        &crate::linear_algebra::UnaryMatrixRequest { operation },
    )?;
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn execute_matrix_solve_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [matrix_node, vector_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(
            "MatrixSolve 需要矩阵和向量".into(),
        ));
    };
    let mut matrix = execute_elaborated_structure(engine, matrix_node)?;
    let mut vector = execute_elaborated_structure(engine, vector_node)?;
    if !matches!(matrix.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, matrix, "MatrixSolve", 0);
    }
    if !matches!(vector.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, vector, "MatrixSolve", 1);
    }
    let mut current = BinarySemanticOperation::compute(
        &crate::linear_algebra::MatrixSolveOperation,
        engine,
        matrix.value().unwrap(),
        vector.value().unwrap(),
        &expression.object.id,
    )?;
    merge_prior_computation(&mut current, &mut matrix);
    merge_prior_computation(&mut current, &mut vector);
    Ok(current)
}

fn execute_factor_projection_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [operand_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(
            "Factors 需要一个矩阵分解对象".into(),
        ));
    };
    let mut operand = execute_elaborated_structure(engine, operand_node)?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, operand, "Factors", 0);
    }
    let input = operand
        .value()
        .expect("checked factorization operand")
        .clone();
    let mut current =
        crate::linear_algebra::FactorProjectionOperation.compute(engine, &input, &())?;
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn execute_substitution_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let [variable, replacement_node, operand_node] = expression.children.as_slice() else {
        return Err(EngineError::InvalidInput(
            "Subst 需要 variable、replacement 和 operand".into(),
        ));
    };
    let mut replacement = execute_elaborated_structure(engine, replacement_node)?;
    let mut operand = execute_elaborated_structure(engine, operand_node)?;
    if matches!(replacement.output, ComputationOutput::EffectsOnly)
        || matches!(operand.output, ComputationOutput::EffectsOnly)
    {
        return Err(EngineError::InvalidInput(
            "带副作用的动作不能参与变量替换".into(),
        ));
    }
    if matches!(replacement.output, ComputationOutput::NoValue(_)) {
        return retain_pending_application(expression, replacement, "Subst", 1);
    }
    if matches!(operand.output, ComputationOutput::NoValue(_)) {
        return retain_pending_application(expression, operand, "Subst", 2);
    }
    let input = operand
        .subject()
        .expect("substitution operand owns an object")
        .clone();
    let replacement_object = replacement
        .subject()
        .expect("substitution replacement owns an object")
        .clone();
    let request = crate::substitution::SubstitutionRequest {
        variable: variable.object.print_source(),
    };
    let mut current = BinarySemanticOperation::compute(
        &crate::substitution::SubstitutionOperation,
        engine,
        &input,
        &replacement_object,
        &request,
    )?;
    merge_prior_computation(&mut current, &mut operand);
    merge_prior_computation(&mut current, &mut replacement);
    Ok(current)
}

fn execute_transform_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let (operand_node, variable) = match (head, expression.children.as_slice()) {
        ("Apart", [operand, variable]) => (operand, Some(variable.object.print_source())),
        (_, [operand]) => (operand, None),
        _ => return Err(EngineError::InvalidInput(format!("{head} 参数数量错误"))),
    };
    let mut operand = execute_elaborated_structure(engine, operand_node)?;
    if !matches!(operand.output, ComputationOutput::Value(_)) {
        return retain_pending_application(expression, operand, head, 0);
    }
    let input = operand.value().expect("checked value operand").clone();
    let kind = match head {
        "Simplify" => crate::algebra::TransformKind::Simplify,
        "Tidy" => crate::algebra::TransformKind::Tidy,
        "Expand" => crate::algebra::TransformKind::Expand,
        "Factor" => crate::algebra::TransformKind::Factor,
        "Apart" => crate::algebra::TransformKind::Apart,
        _ => unreachable!(),
    };
    let mut current = crate::algebra::TransformOperation.compute(
        engine,
        &input,
        &crate::algebra::TransformRequest { kind, variable },
    )?;
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn execute_container(
    expression: &crate::elaboration::ElaboratedObject,
    engine: &mut dyn Engine,
) -> Result<Computation, EngineError> {
    let mut children = expression
        .children
        .iter()
        .map(|child| execute_elaborated_structure(engine, child))
        .collect::<Result<Vec<_>, _>>()?;
    if children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::EffectsOnly))
    {
        return Err(EngineError::InvalidInput(
            "带副作用的动作不能作为关系或集合的成员".into(),
        ));
    }
    let members: Vec<_> = children
        .iter()
        .map(|child| {
            child
                .subject()
                .expect("container member owns a mathematical object")
                .clone()
        })
        .collect();
    let rebuilt = expression.object.rebuild_application_with(&members)?;
    let conditions = conditions_from_objects(&members)?;
    let exactness = exactness_from_objects(&members);
    let no_value = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::NoValue(_)));
    let held = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::Held(_)));
    let kind = crate::input::with_parse_env(|env| {
        crate::semantic::analyze_tree(env, &rebuilt).semantic.kind
    });
    let (interpretation, rule) = match &expression.form {
        crate::elaboration::MathematicalForm::Relation { operator } => (
            SemanticInterpretation::Equation,
            format!("construct-relation-{operator}"),
        ),
        crate::elaboration::MathematicalForm::Collection => {
            match &expression.object.semantics.interpretation {
                SemanticInterpretation::Matrix { rows, columns } => (
                    SemanticInterpretation::Matrix {
                        rows: *rows,
                        columns: *columns,
                    },
                    "construct-matrix".into(),
                ),
                _ => (SemanticInterpretation::List, "construct-collection".into()),
            }
        }
        _ => unreachable!(),
    };
    let metadata = if no_value {
        metadata_with_conditions(
            ResultMetadata::no_result(exactness, OutcomeReason::MathematicalAbsence),
            &conditions,
        )
    } else if held {
        metadata_with_conditions(
            ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered),
            &conditions,
        )
    } else {
        ResultMetadata::solved(exactness, conditions.clone())
    };
    let mut output = expression.object.clone();
    output.apply(ObjectDelta {
        expression: Some(rebuilt),
        semantics: Some(SemanticState {
            kind: if no_value || held {
                ValueKind::Unevaluated
            } else {
                kind
            },
            interpretation,
            metadata,
            capabilities: if no_value {
                CapabilitySet::empty()
            } else {
                expression.object.semantics.capabilities
            },
            requirements: Vec::new(),
        }),
        overlay: None,
        normalization: (!no_value && !held).then_some(NormalizationMetadata {
            level: NormalizationLevel::Structural,
            assumptions: conditions.conditions().to_vec(),
            mode: NormalizationMode::Safe,
        }),
    });
    let mut events = Vec::new();
    for child in &mut children {
        if let Some(trace) = child.trace.take() {
            events.extend(trace.events);
        }
    }
    events.push(RuleEvent {
        rule,
        input: members
            .first()
            .map(|member| member.reference(None))
            .unwrap_or_else(|| expression.object.reference(None)),
        additional_inputs: members
            .iter()
            .skip(1)
            .map(|member| member.reference(None))
            .collect(),
        output: output.reference(None),
        bindings: Vec::new(),
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Structural,
        importance: RuleImportance::Normal,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "用已类型化的成员重建数学容器。".into(),
            tex_override: None,
        }),
    });
    Ok(Computation {
        output: if no_value {
            ComputationOutput::NoValue(output)
        } else if held {
            ComputationOutput::Held(output)
        } else {
            ComputationOutput::Value(output)
        },
        trace: Some(RuleTrace { events }),
        certificates: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.certificates))
            .collect(),
        effects: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.effects))
            .collect(),
    })
}

fn execute_function_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let mut children = expression
        .children
        .iter()
        .map(|child| execute_elaborated_structure(engine, child))
        .collect::<Result<Vec<_>, _>>()?;
    if children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::EffectsOnly))
    {
        return Err(EngineError::InvalidInput(format!(
            "带副作用的动作不能作为 {head} 的参数"
        )));
    }
    let arguments: Vec<_> = children
        .iter()
        .map(|child| {
            child
                .subject()
                .expect("mathematical function argument owns an object")
                .clone()
        })
        .collect();
    let rebuilt = expression.object.rebuild_application_with(&arguments)?;
    let conditions = conditions_from_objects(&arguments)?;
    let exactness = exactness_from_objects(&arguments);
    let no_value = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::NoValue(_)));
    let held = children
        .iter()
        .any(|child| matches!(child.output, ComputationOutput::Held(_)));
    let mut output = expression.object.clone();
    let semantics = if no_value {
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::StructuredUnevaluated {
                reason: "argument has no mathematical value".into(),
            },
            metadata: metadata_with_conditions(
                ResultMetadata::no_result(exactness, OutcomeReason::MathematicalAbsence),
                &conditions,
            ),
            capabilities: CapabilitySet::empty(),
            requirements: Vec::new(),
        }
    } else if held {
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::HeldApplication {
                operator: head.into(),
            },
            metadata: metadata_with_conditions(
                ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered),
                &conditions,
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }
    } else {
        let rebuilt_object = crate::semantic_core::MathematicalObject::new(
            expression.object.id,
            rebuilt.clone(),
            expression.object.semantics.clone(),
        );
        let evaluated = engine
            .eval_expr(&rebuilt_object.print_source())?
            .to_string();
        let evaluated_object = object_from_source(
            expression.object.id,
            &evaluated,
            expression.object.semantics.clone(),
        )?;
        let kind = crate::input::with_parse_env(|env| {
            crate::semantic::analyze_tree(env, &evaluated_object.raw_expression())
                .semantic
                .kind
        });
        output = evaluated_object;
        SemanticState {
            kind,
            interpretation: SemanticInterpretation::PlainExpression,
            metadata: ResultMetadata::solved(exactness, conditions.clone()),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        }
    };
    output.apply(ObjectDelta {
        expression: (!matches!(semantics.metadata.resolution, ResolutionState::Solved))
            .then_some(rebuilt),
        semantics: Some(semantics),
        overlay: None,
        normalization: (!held && !no_value).then_some(NormalizationMetadata {
            level: NormalizationLevel::Structural,
            assumptions: conditions.conditions().to_vec(),
            mode: NormalizationMode::Safe,
        }),
    });
    let rule = if no_value {
        "propagate-no-value"
    } else if held {
        "hold-function-application"
    } else {
        "apply-function"
    };
    let event = RuleEvent {
        rule: rule.into(),
        input: arguments
            .first()
            .map(|argument| argument.reference(None))
            .unwrap_or_else(|| expression.object.reference(None)),
        additional_inputs: arguments
            .iter()
            .skip(1)
            .map(|argument| argument.reference(None))
            .collect(),
        output: output.reference(None),
        bindings: vec![("function".into(), head.into())],
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Rewrite,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "将已类型化的参数应用到数学函数。".into(),
            tex_override: None,
        }),
    };
    let mut events = Vec::new();
    for child in &mut children {
        if let Some(trace) = child.trace.take() {
            events.extend(trace.events);
        }
    }
    events.push(event);
    Ok(Computation {
        output: if no_value {
            ComputationOutput::NoValue(output)
        } else if held {
            ComputationOutput::Held(output)
        } else {
            ComputationOutput::Value(output)
        },
        trace: Some(RuleTrace { events }),
        certificates: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.certificates))
            .collect(),
        effects: children
            .iter_mut()
            .flat_map(|child| std::mem::take(&mut child.effects))
            .collect(),
    })
}

fn execute_calculus_application(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    head: &str,
) -> Result<Computation, EngineError> {
    let (operand_index, mut request) = match head {
        "Limit" => {
            let (operand_index, variable, at, direction) = match expression.children.as_slice() {
                [_operand, at] => (
                    0,
                    "x".to_string(),
                    at.object.print_source(),
                    crate::limits::LimitDirection::Both,
                ),
                [variable, at, _operand] => (
                    2,
                    variable.object.print_source(),
                    at.object.print_source(),
                    crate::limits::LimitDirection::Both,
                ),
                [variable, at, direction, _operand] => {
                    let direction = match direction.object.print_source().as_str() {
                        "Left" => crate::limits::LimitDirection::Left,
                        "Right" => crate::limits::LimitDirection::Right,
                        _ => {
                            return Err(EngineError::InvalidInput(
                                "极限方向应为 Left 或 Right".into(),
                            ))
                        }
                    };
                    (
                        3,
                        variable.object.print_source(),
                        at.object.print_source(),
                        direction,
                    )
                }
                _ => {
                    return Err(EngineError::InvalidInput(format!(
                        "Limit 不支持 {} 个参数",
                        expression.children.len()
                    )))
                }
            };
            (
                operand_index,
                CalculusRequest::Limit(crate::limits::LimitRequest {
                    variable,
                    at,
                    direction,
                }),
            )
        }
        "D" | "Deriv" => {
            let (operand_index, variable, order) =
                match expression.children.as_slice() {
                    [variable, _operand] => (1, variable.object.print_source(), 1),
                    [variable, order, _operand] => (
                        2,
                        variable.object.print_source(),
                        order.object.print_source().parse::<u32>().map_err(|_| {
                            EngineError::InvalidInput("求导阶数必须是正整数".into())
                        })?,
                    ),
                    _ => {
                        return Err(EngineError::InvalidInput(format!(
                            "{head} 不支持 {} 个参数",
                            expression.children.len()
                        )))
                    }
                };
            (
                operand_index,
                CalculusRequest::Derivative(crate::derivatives::DerivativeRequest {
                    variable,
                    order,
                }),
            )
        }
        "Integrate" => match expression.children.as_slice() {
            [variable, _operand] => (
                1,
                CalculusRequest::Integral(crate::integrals::IntegralRequest {
                    variable: variable.object.print_source(),
                    arbitrary_constant: "C".into(),
                }),
            ),
            [variable, lower, upper, _operand] => (
                3,
                CalculusRequest::DefiniteIntegral(crate::integrals::DefiniteIntegralRequest {
                    variable: variable.object.print_source(),
                    lower: lower.object.print_source(),
                    upper: upper.object.print_source(),
                }),
            ),
            _ => {
                return Err(EngineError::InvalidInput(format!(
                    "Integrate 不支持 {} 个参数",
                    expression.children.len()
                )))
            }
        },
        _ => unreachable!(),
    };
    if let CalculusRequest::Derivative(derivative) = &request {
        let operand_node = &expression.children[operand_index];
        let typed_integral = matches!(
            &operand_node.object.semantics.interpretation,
            SemanticInterpretation::TypedApplication(application)
                | SemanticInterpretation::HeldTypedApplication(application)
                if application.operator == OperatorId::Integral
        );
        if typed_integral {
            if let Some(mut lowered) = crate::lowering::try_lower_application(
                engine,
                &operand_node.object,
                crate::semantic_core::TraceMode::Detailed,
            )? {
                let input = lowered
                    .value()
                    .expect("successful integral lowering produces a value")
                    .clone();
                let mut computation =
                    crate::derivatives::DerivativeOperation.compute(engine, &input, derivative)?;
                merge_prior_computation(&mut computation, &mut lowered);
                return Ok(computation);
            }
            return crate::derivatives::DerivativeOperation.compute(
                engine,
                &operand_node.object,
                derivative,
            );
        }
    }
    let mut operand = execute_elaborated_structure(engine, &expression.children[operand_index])?;
    let derivative_accepts_held = matches!(request, CalculusRequest::Derivative(_))
        && matches!(
            operand
                .subject()
                .map(|object| &object.semantics.interpretation),
            Some(SemanticInterpretation::HeldTypedApplication(_))
        );
    if !matches!(operand.output, ComputationOutput::Value(_)) && !derivative_accepts_held {
        return retain_pending_application(expression, operand, head, operand_index);
    }
    let input = operand
        .subject()
        .expect("checked mathematical computation")
        .clone();
    if let CalculusRequest::Integral(request) = &mut request {
        request.arbitrary_constant = crate::integrals::available_constant(&input);
    }
    let required_capability = match &request {
        CalculusRequest::Limit(_) => ObjectCapability::EvaluateLimit,
        CalculusRequest::Derivative(_) => ObjectCapability::Differentiate,
        CalculusRequest::Integral(_) => ObjectCapability::Integrate,
        CalculusRequest::DefiniteIntegral(_) => ObjectCapability::Integrate,
    };
    if !input.semantics.capabilities.contains(required_capability) {
        return retain_pending_application(expression, operand, head, operand_index);
    }
    let mut computation = match request {
        CalculusRequest::Limit(request) => {
            crate::limits::LimitOperation.compute(engine, &input, &request)?
        }
        CalculusRequest::Derivative(request) => {
            crate::derivatives::DerivativeOperation.compute(engine, &input, &request)?
        }
        CalculusRequest::Integral(request) => {
            crate::integrals::IntegralOperation.compute(engine, &input, &request)?
        }
        CalculusRequest::DefiniteIntegral(request) => {
            crate::integrals::DefiniteIntegralOperation.compute(engine, &input, &request)?
        }
    };
    merge_prior_computation(&mut computation, &mut operand);
    Ok(computation)
}

enum CalculusRequest {
    Limit(crate::limits::LimitRequest),
    Derivative(crate::derivatives::DerivativeRequest),
    Integral(crate::integrals::IntegralRequest),
    DefiniteIntegral(crate::integrals::DefiniteIntegralRequest),
}

fn retain_pending_application(
    expression: &crate::elaboration::ElaboratedObject,
    mut operand: Computation,
    head: &str,
    operand_index: usize,
) -> Result<Computation, EngineError> {
    if matches!(operand.output, ComputationOutput::EffectsOnly) {
        return Err(EngineError::InvalidInput(format!(
            "带副作用的动作不能作为 {head} 的 operand"
        )));
    }
    let child = operand
        .subject()
        .expect("non-effect application operand owns an object")
        .clone();
    let mut arguments = expression
        .children
        .iter()
        .map(|node| node.object.clone())
        .collect::<Vec<_>>();
    arguments[operand_index] = child.clone();
    let rebuilt = expression.object.rebuild_application_with(&arguments)?;
    let no_value = matches!(operand.output, ComputationOutput::NoValue(_));
    let mut output = expression.object.clone();
    let held_interpretation = match &expression.object.semantics.interpretation {
        SemanticInterpretation::TypedApplication(application) => {
            let mut application = application.clone();
            application.conditions = child.semantics.metadata.conditions.clone();
            SemanticInterpretation::HeldTypedApplication(application)
        }
        _ => SemanticInterpretation::HeldApplication {
            operator: head.into(),
        },
    };
    output.apply(ObjectDelta {
        expression: Some(rebuilt),
        semantics: Some(SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: if no_value {
                SemanticInterpretation::StructuredUnevaluated {
                    reason: "operand has no mathematical value".into(),
                }
            } else {
                held_interpretation
            },
            metadata: if no_value {
                ResultMetadata::no_result(
                    child.semantics.metadata.exactness,
                    OutcomeReason::MathematicalAbsence,
                )
            } else {
                ResultMetadata::unresolved(
                    child.semantics.metadata.exactness,
                    OutcomeReason::AlgorithmUncovered,
                )
            },
            capabilities: if no_value {
                CapabilitySet::empty()
            } else {
                CapabilitySet::symbolic_expression()
            },
            requirements: Vec::new(),
        }),
        overlay: None,
        normalization: None,
    });
    let event = RuleEvent {
        rule: if no_value {
            "propagate-no-value"
        } else {
            "hold-operator-application"
        }
        .into(),
        input: child.reference(None),
        additional_inputs: Vec::new(),
        output: output.reference(None),
        bindings: vec![("operation".into(), head.into())],
        conditions: Vec::new(),
        payload: RulePayload::Structural,
        importance: RuleImportance::Key,
        presentation: None,
    };
    let mut current = Computation {
        output: if no_value {
            ComputationOutput::NoValue(output)
        } else {
            ComputationOutput::Held(output)
        },
        trace: Some(RuleTrace {
            events: vec![event],
        }),
        certificates: Vec::new(),
        effects: Vec::new(),
    };
    merge_prior_computation(&mut current, &mut operand);
    Ok(current)
}

fn merge_prior_computation(current: &mut Computation, prior: &mut Computation) {
    let mut events = prior
        .trace
        .take()
        .map(|trace| trace.events)
        .unwrap_or_default();
    if let Some(trace) = current.trace.as_mut() {
        events.append(&mut trace.events);
        trace.events = events;
    } else if !events.is_empty() {
        current.trace = Some(RuleTrace { events });
    }
    current.certificates.append(&mut prior.certificates);
    current.effects.append(&mut prior.effects);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Negate,
}

#[derive(Debug, Clone, Copy)]
pub struct ArithmeticRequest {
    pub output_id: ObjectId,
    pub operation: ArithmeticOperation,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ArithmeticOperationExecutor;

impl BinarySemanticOperation<ArithmeticRequest> for ArithmeticOperationExecutor {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        left: &crate::semantic_core::MathematicalObject,
        right: &crate::semantic_core::MathematicalObject,
        request: &ArithmeticRequest,
    ) -> Result<Computation, EngineError> {
        if request.operation == ArithmeticOperation::Negate {
            return Err(EngineError::InvalidInput(
                "一元负号不能作为二元运算执行".into(),
            ));
        }
        let symbol = symbol(request.operation);
        let source = format!(
            "({}){symbol}({})",
            left.print_source(),
            right.print_source()
        );
        if left.semantics.metadata.resolution == ResolutionState::NoResult
            || right.semantics.metadata.resolution == ResolutionState::NoResult
        {
            return no_value_structure(
                request.output_id,
                &source,
                left,
                Some(right),
                request.operation,
            );
        }
        let capability = capability(request.operation);
        if !left.semantics.capabilities.contains(capability)
            || !right.semantics.capabilities.contains(capability)
        {
            return Err(EngineError::InvalidInput(
                "数学对象不具备该结构运算能力".into(),
            ));
        }
        let held = left.semantics.metadata.resolution == ResolutionState::Unresolved
            || right.semantics.metadata.resolution == ResolutionState::Unresolved;
        let conditionally_sensitive = matches!(
            request.operation,
            ArithmeticOperation::Divide | ArithmeticOperation::Power
        ) && (left.semantics.kind != ValueKind::Scalar
            || right.semantics.kind != ValueKind::Scalar);
        let output_source = if held || conditionally_sensitive {
            source
        } else {
            engine.eval_expr(&source)?.to_string()
        };
        let conditions = combined_conditions(left, Some(right))?;
        let mut semantics = SemanticState {
            kind: if held {
                ValueKind::Unevaluated
            } else {
                ValueKind::Expression
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: symbol.into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: arithmetic_metadata(
                held,
                combined_exactness(left, Some(right)),
                conditions.clone(),
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let mut output = object_from_source(request.output_id, &output_source, semantics.clone())?;
        if !held {
            semantics.kind = crate::input::with_parse_env(|env| {
                crate::semantic::analyze_tree(env, &output.raw_expression())
                    .semantic
                    .kind
            });
            output.apply(ObjectDelta {
                expression: None,
                semantics: Some(semantics),
                overlay: None,
                normalization: Some(NormalizationMetadata {
                    level: NormalizationLevel::Structural,
                    assumptions: conditions.conditions().to_vec(),
                    mode: NormalizationMode::Safe,
                }),
            });
        }
        let event = RuleEvent {
            rule: match request.operation {
                ArithmeticOperation::Add => "add",
                ArithmeticOperation::Subtract => "subtract",
                ArithmeticOperation::Multiply => "multiply",
                ArithmeticOperation::Divide => "divide",
                ArithmeticOperation::Power => "power",
                ArithmeticOperation::Negate => unreachable!(),
            }
            .into(),
            input: left.reference(None),
            additional_inputs: vec![right.reference(None)],
            output: output.reference(None),
            bindings: Vec::new(),
            conditions: conditions.conditions().to_vec(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: output.print_source(),
                explanation: "组合两个已类型化的数学对象。".into(),
                tex_override: None,
            }),
        };
        Ok(Computation {
            output: if held {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

impl UnarySemanticOperation<ArithmeticRequest> for ArithmeticOperationExecutor {
    fn compute(
        &self,
        engine: &mut dyn Engine,
        input: &crate::semantic_core::MathematicalObject,
        request: &ArithmeticRequest,
    ) -> Result<Computation, EngineError> {
        if request.operation != ArithmeticOperation::Negate {
            return Err(EngineError::InvalidInput("该算术请求不是一元运算".into()));
        }
        if input.semantics.metadata.resolution == ResolutionState::NoResult {
            return no_value_structure(
                request.output_id,
                &format!("-({})", input.print_source()),
                input,
                None,
                request.operation,
            );
        }
        if !input
            .semantics
            .capabilities
            .contains(ObjectCapability::Negate)
        {
            return Err(EngineError::InvalidInput(
                "数学对象不具备一元取负能力".into(),
            ));
        }
        let source = format!("-({})", input.print_source());
        let held = input.semantics.metadata.resolution == ResolutionState::Unresolved;
        let output_source = if held {
            source
        } else {
            engine.eval_expr(&source)?.to_string()
        };
        let conditions = combined_conditions(input, None)?;
        let mut semantics = SemanticState {
            kind: if held {
                ValueKind::Unevaluated
            } else {
                ValueKind::Expression
            },
            interpretation: if held {
                SemanticInterpretation::HeldApplication {
                    operator: "-".into(),
                }
            } else {
                SemanticInterpretation::PlainExpression
            },
            metadata: arithmetic_metadata(
                held,
                combined_exactness(input, None),
                conditions.clone(),
            ),
            capabilities: CapabilitySet::symbolic_expression(),
            requirements: Vec::new(),
        };
        let mut output = object_from_source(request.output_id, &output_source, semantics.clone())?;
        if !held {
            semantics.kind = crate::input::with_parse_env(|env| {
                crate::semantic::analyze_tree(env, &output.raw_expression())
                    .semantic
                    .kind
            });
            output.apply(ObjectDelta {
                expression: None,
                semantics: Some(semantics),
                overlay: None,
                normalization: Some(NormalizationMetadata {
                    level: NormalizationLevel::Structural,
                    assumptions: conditions.conditions().to_vec(),
                    mode: NormalizationMode::Safe,
                }),
            });
        }
        let event = RuleEvent {
            rule: "negate".into(),
            input: input.reference(None),
            additional_inputs: Vec::new(),
            output: output.reference(None),
            bindings: Vec::new(),
            conditions: conditions.conditions().to_vec(),
            payload: RulePayload::Rewrite,
            importance: RuleImportance::Key,
            presentation: Some(RulePresentation {
                expression: output.print_source(),
                explanation: "对已类型化的数学对象取负。".into(),
                tex_override: None,
            }),
        };
        Ok(Computation {
            output: if held {
                ComputationOutput::Held(output)
            } else {
                ComputationOutput::Value(output)
            },
            trace: Some(RuleTrace {
                events: vec![event],
            }),
            certificates: Vec::new(),
            effects: Vec::new(),
        })
    }
}

fn capability(operation: ArithmeticOperation) -> ObjectCapability {
    match operation {
        ArithmeticOperation::Add => ObjectCapability::Add,
        ArithmeticOperation::Subtract => ObjectCapability::Subtract,
        ArithmeticOperation::Multiply => ObjectCapability::Multiply,
        ArithmeticOperation::Divide => ObjectCapability::Divide,
        ArithmeticOperation::Power => ObjectCapability::Power,
        ArithmeticOperation::Negate => ObjectCapability::Negate,
    }
}

fn symbol(operation: ArithmeticOperation) -> &'static str {
    match operation {
        ArithmeticOperation::Add => "+",
        ArithmeticOperation::Subtract => "-",
        ArithmeticOperation::Multiply => "*",
        ArithmeticOperation::Divide => "/",
        ArithmeticOperation::Power => "^",
        ArithmeticOperation::Negate => "-",
    }
}

fn combined_conditions(
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
) -> Result<ConditionSet, EngineError> {
    let conditions: Vec<Condition> = left
        .semantics
        .metadata
        .conditions
        .conditions()
        .iter()
        .chain(
            right
                .into_iter()
                .flat_map(|object| object.semantics.metadata.conditions.conditions().iter()),
        )
        .cloned()
        .collect();
    ConditionSet::new(conditions)
}

fn conditions_from_objects(
    objects: &[crate::semantic_core::MathematicalObject],
) -> Result<ConditionSet, EngineError> {
    ConditionSet::new(objects.iter().flat_map(|object| {
        object
            .semantics
            .metadata
            .conditions
            .conditions()
            .iter()
            .cloned()
    }))
}

fn combined_exactness(
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
) -> Exactness {
    let mut exactness = left.semantics.metadata.exactness;
    if let Some(right) = right {
        exactness = match (exactness, right.semantics.metadata.exactness) {
            (Exactness::Approximate, _) | (_, Exactness::Approximate) => Exactness::Approximate,
            (Exactness::Unknown, _) | (_, Exactness::Unknown) => Exactness::Unknown,
            (Exactness::Symbolic, _) | (_, Exactness::Symbolic) => Exactness::Symbolic,
            (Exactness::Exact, Exactness::Exact) => Exactness::Exact,
        };
    }
    exactness
}

fn exactness_from_objects(objects: &[crate::semantic_core::MathematicalObject]) -> Exactness {
    objects.iter().fold(Exactness::Exact, |combined, object| {
        match (combined, object.semantics.metadata.exactness) {
            (Exactness::Approximate, _) | (_, Exactness::Approximate) => Exactness::Approximate,
            (Exactness::Unknown, _) | (_, Exactness::Unknown) => Exactness::Unknown,
            (Exactness::Symbolic, _) | (_, Exactness::Symbolic) => Exactness::Symbolic,
            (Exactness::Exact, Exactness::Exact) => Exactness::Exact,
        }
    })
}

fn metadata_with_conditions(
    mut metadata: ResultMetadata,
    conditions: &ConditionSet,
) -> ResultMetadata {
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions.clone();
    }
    metadata
}

fn arithmetic_metadata(
    held: bool,
    exactness: Exactness,
    conditions: ConditionSet,
) -> ResultMetadata {
    if !held {
        return ResultMetadata::solved(exactness, conditions);
    }
    let mut metadata = ResultMetadata::unresolved(exactness, OutcomeReason::AlgorithmUncovered);
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions;
    }
    metadata
}

fn no_value_structure(
    output_id: ObjectId,
    source: &str,
    left: &crate::semantic_core::MathematicalObject,
    right: Option<&crate::semantic_core::MathematicalObject>,
    operation: ArithmeticOperation,
) -> Result<Computation, EngineError> {
    let conditions = combined_conditions(left, right)?;
    let mut metadata = ResultMetadata::no_result(
        combined_exactness(left, right),
        OutcomeReason::MathematicalAbsence,
    );
    if !conditions.is_empty() {
        metadata.conditionality = Conditionality::Conditional;
        metadata.conditions = conditions.clone();
    }
    let output = object_from_source(
        output_id,
        source,
        SemanticState {
            kind: ValueKind::Unevaluated,
            interpretation: SemanticInterpretation::StructuredUnevaluated {
                reason: "operand has no mathematical value".into(),
            },
            metadata,
            capabilities: CapabilitySet::empty(),
            requirements: Vec::new(),
        },
    )?;
    let event = RuleEvent {
        rule: "propagate-no-value".into(),
        input: left.reference(None),
        additional_inputs: right
            .into_iter()
            .map(|object| object.reference(None))
            .collect(),
        output: output.reference(None),
        bindings: vec![("operator".into(), symbol(operation).into())],
        conditions: conditions.conditions().to_vec(),
        payload: RulePayload::Inference,
        importance: RuleImportance::Key,
        presentation: Some(RulePresentation {
            expression: output.print_source(),
            explanation: "操作数没有数学值，因此结构运算也没有值。".into(),
            tex_override: None,
        }),
    };
    Ok(Computation {
        output: ComputationOutput::NoValue(output),
        trace: Some(RuleTrace {
            events: vec![event],
        }),
        certificates: Vec::new(),
        effects: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;
    use crate::semantic_core::{ObjectId, SemanticState};

    fn expression(
        id: u64,
        source: &str,
        resolution: ResolutionState,
    ) -> crate::semantic_core::MathematicalObject {
        object_from_source(
            ObjectId(id),
            source,
            SemanticState {
                kind: ValueKind::Expression,
                interpretation: SemanticInterpretation::PlainExpression,
                metadata: match resolution {
                    ResolutionState::Solved => {
                        ResultMetadata::solved(Exactness::Symbolic, ConditionSet::empty())
                    }
                    ResolutionState::Unresolved => ResultMetadata::unresolved(
                        Exactness::Symbolic,
                        OutcomeReason::AlgorithmUncovered,
                    ),
                    ResolutionState::NoResult => ResultMetadata::no_result(
                        Exactness::Symbolic,
                        OutcomeReason::MathematicalAbsence,
                    ),
                },
                capabilities: CapabilitySet::symbolic_expression(),
                requirements: Vec::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn combines_two_typed_objects_and_records_both_provenances() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            &mut engine,
            &expression(1, "x", ResolutionState::Solved),
            &expression(2, "x", ResolutionState::Solved),
            &ArithmeticRequest {
                output_id: ObjectId(3),
                operation: ArithmeticOperation::Add,
            },
        )
        .unwrap();
        assert_eq!(result.value().unwrap().print_source(), "2*x");
        let event = &result.trace.as_ref().unwrap().events[0];
        assert_eq!(event.input.object, ObjectId(1));
        assert_eq!(event.additional_inputs[0].object, ObjectId(2));
    }

    #[test]
    fn preserves_an_unresolved_operand_without_engine_lowering() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = BinarySemanticOperation::compute(
            &ArithmeticOperationExecutor,
            &mut engine,
            &expression(1, "Limit(x,0)(f(x))", ResolutionState::Unresolved),
            &expression(2, "x", ResolutionState::Solved),
            &ArithmeticRequest {
                output_id: ObjectId(3),
                operation: ArithmeticOperation::Multiply,
            },
        )
        .unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result.subject().unwrap().print_source().contains("Limit"));
    }

    #[test]
    fn recursively_executes_every_structural_operator() {
        let mut engine = RustEngine::spawn().unwrap();
        let numeric = crate::elaboration::elaborate("2+3*4").unwrap();
        let result = execute_elaborated_structure(&mut engine, &numeric).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "14");
        assert_eq!(result.trace.as_ref().unwrap().events.len(), 2);

        for source in ["x-1", "x/2", "x^2", "-(x-1)"] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert!(
                matches!(result.output, ComputationOutput::Value(_)),
                "{source}"
            );
            let output = result.value().unwrap();
            assert_eq!(
                output.normalization.as_ref().unwrap().metadata.level,
                NormalizationLevel::Structural
            );
        }
    }

    #[test]
    fn structural_division_does_not_apply_conditional_cancellation() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("x/x").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "x/x");
        assert!(result
            .value()
            .unwrap()
            .normalization
            .as_ref()
            .unwrap()
            .metadata
            .assumptions
            .is_empty());
    }

    #[test]
    fn arithmetic_declares_its_minimum_input_normalization() {
        assert_eq!(
            BinarySemanticOperation::<ArithmeticRequest>::minimum_input_normalization(
                &ArithmeticOperationExecutor
            ),
            NormalizationLevel::Structural
        );
        assert_eq!(
            UnarySemanticOperation::<ArithmeticRequest>::minimum_input_normalization(
                &ArithmeticOperationExecutor
            ),
            NormalizationLevel::Structural
        );
    }

    #[test]
    fn recursively_executes_limit_and_derivative_inside_structures() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("(Limit(t,0)(Sin(t)/t+x^2))+3").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::Value(_)));
        assert_eq!(
            engine
                .eval_expr(&format!(
                    "Simplify(({})-(x^2+4))",
                    result.value().unwrap().print_source()
                ))
                .unwrap()
                .to_string(),
            "0"
        );
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "limit-result"));

        let chain = crate::elaboration::elaborate("D(x)((Limit(t,0)(Sin(t)/t+x^2)))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &chain).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "2*x");
        assert!(result.trace.as_ref().unwrap().events.len() >= 2);
    }

    #[test]
    fn no_value_propagates_through_outer_structures() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("(Limit(x,0)(1/x))+1").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::NoValue(_)));
        assert_eq!(
            result.subject().unwrap().semantics.metadata.resolution,
            ResolutionState::NoResult
        );
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "propagate-no-value"));
    }

    #[test]
    fn generic_functions_consume_nested_semantic_objects() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("Sin(Limit(t,0)(Sin(t)/t))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "Sin(1)");
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "apply-function"));

        let chain = crate::elaboration::elaborate("D(x)(Sin(Limit(t,0)(Sin(t)/t+x)))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &chain).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "Cos(x+1)");
    }

    #[test]
    fn generic_functions_propagate_no_value_without_evaluation() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("Sin(Limit(x,0)(1/x))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::NoValue(_)));
        assert!(result.subject().unwrap().print_source().contains("Limit"));
    }

    #[test]
    fn effects_are_rejected_as_function_or_structural_operands() {
        let mut engine = RustEngine::spawn().unwrap();
        for source in ["Sin(Plot(x,x,0,1))", "(Plot(x,x,0,1))+1"] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let error = match execute_elaborated_structure(&mut engine, &elaborated) {
                Ok(_) => panic!("{source} should reject an effect operand"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("副作用"), "{source}: {error}");
        }
    }

    #[test]
    fn collections_and_relations_rebuild_from_semantic_children() {
        let mut engine = RustEngine::spawn().unwrap();
        let list = crate::elaboration::elaborate("{Limit(t,0)(Sin(t)/t),D(x)(x^2)}").unwrap();
        let result = execute_elaborated_structure(&mut engine, &list).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "{1,2*x}");

        let matrix = crate::elaboration::elaborate("{{Limit(t,0)(Sin(t)/t),2},{3,4}}").unwrap();
        let result = execute_elaborated_structure(&mut engine, &matrix).unwrap();
        assert_eq!(result.value().unwrap().semantics.kind, ValueKind::Matrix);
        assert_eq!(result.value().unwrap().print_source(), "{{1,2},{3,4}}");

        let relation = crate::elaboration::elaborate("(Limit(t,0)(Sin(t)/t))==1").unwrap();
        let result = execute_elaborated_structure(&mut engine, &relation).unwrap();
        assert_eq!(result.value().unwrap().semantics.kind, ValueKind::Equation);
        assert_eq!(result.value().unwrap().print_source(), "1==1");
    }

    #[test]
    fn derivative_peels_even_an_unresolved_indefinite_integral() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("D(x)(Integrate(x)f(x))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert_eq!(result.value().unwrap().print_source(), "f(x)");
        assert!(result
            .trace
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.rule == "derivative-of-indefinite-integral"));
    }

    #[test]
    fn definite_integrals_compose_as_typed_values() {
        let mut engine = RustEngine::spawn().unwrap();
        for (source, expected) in [
            ("Integrate(x,0,1)(x^2)", "1/3"),
            ("2+Integrate(x,0,1)(x^2)", "7/3"),
            ("Sin(Integrate(x,0,1)(x^2))", "Sin(1/3)"),
        ] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            assert!(can_execute_elaborated_tree(&elaborated), "{source}");
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert_eq!(result.value().unwrap().print_source(), expected, "{source}");
        }
    }

    #[test]
    fn algebra_transforms_compose_with_object_native_calculus() {
        let mut engine = RustEngine::spawn().unwrap();
        for (source, expected) in [
            ("Factor(D(x)(Integrate(x)(x^2-1)))", "(x+1)*(x-1)"),
            ("Simplify(Integrate(x,0,1)(D(x)(x^3)))", "1"),
            ("Expand(Limit(t,0)((t+1)^2+x))", "x+1"),
            ("Sin(Factor(x^2-1))", "Sin((x+1)*(x-1))"),
        ] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            assert!(can_execute_elaborated_tree(&elaborated), "{source}");
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert_eq!(result.value().unwrap().print_source(), expected, "{source}");
            assert!(result
                .trace
                .as_ref()
                .is_some_and(|trace| trace.events.iter().any(|event| matches!(
                    event.rule.as_str(),
                    "apply-algebra-transform" | "confirm-algebra-normal-form"
                ))));
        }
    }

    #[test]
    fn special_functions_stay_compact_across_algebra_and_calculus_composition() {
        let mut engine = RustEngine::spawn().unwrap();
        for source in [
            "Expand(Gamma(x))",
            "Simplify(D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t))))",
            "Expand(D(x)(Integrate(t,0,Infinity)(t^(x-1)*Exp(-t))))",
        ] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            let output = result.value().unwrap();
            let compact = output.print_source().replace(' ', "");
            assert!(compact.contains("Gamma(x)"), "{source}: {compact}");
            assert!(!compact.contains("Integrate("), "{source}: {compact}");
            assert!(output.stable_representation_count() <= 4);
        }
    }

    #[test]
    fn unresolved_transform_retains_its_outer_application() {
        let mut engine = RustEngine::spawn().unwrap();
        let elaborated = crate::elaboration::elaborate("Factor(Integrate(x)f(x))").unwrap();
        let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .starts_with("Factor("));
        assert!(result
            .subject()
            .unwrap()
            .print_source()
            .contains("Integrate"));
    }

    #[test]
    fn substitution_composes_with_calculus_and_held_objects() {
        let mut engine = RustEngine::spawn().unwrap();
        for (source, expected) in [
            ("Subst(x,2)(D(x)(x^3))", "12"),
            ("D(x)(Subst(y,x^2)(Sin(y)))", "2*Cos(x^2)*x"),
            ("Subst(x,Limit(t,0)(Sin(t)/t))(x^2+1)", "2"),
        ] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            assert!(can_execute_elaborated_tree(&elaborated), "{source}");
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert_eq!(result.value().unwrap().print_source(), expected, "{source}");
            assert!(result
                .trace
                .as_ref()
                .unwrap()
                .events
                .iter()
                .any(|event| event.rule == "substitute-free-symbol"));
        }

        let held = crate::elaboration::elaborate("Subst(x,2)((Integrate(t)f(t))+x)").unwrap();
        let result = execute_elaborated_structure(&mut engine, &held).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        let source = result.subject().unwrap().print_source();
        assert!(source.contains("Integrate"));
        assert!(source.contains("+2"), "{source}");
    }

    #[test]
    fn numeric_evaluation_composes_and_retains_symbolic_functions() {
        let mut engine = RustEngine::spawn().unwrap();
        for source in [
            "N(Integrate(x,0,1)(x^2),20)",
            "N(Subst(x,2)(Sqrt(x)),30)",
            "Sin(N(Pi,20))",
            "N(Pi,20)+1",
        ] {
            let elaborated = crate::elaboration::elaborate(source).unwrap();
            assert!(can_execute_elaborated_tree(&elaborated), "{source}");
            let result = execute_elaborated_structure(&mut engine, &elaborated).unwrap();
            assert!(
                matches!(result.output, ComputationOutput::Value(_)),
                "{source}"
            );
            assert_eq!(
                result.value().unwrap().semantics.metadata.resolution,
                ResolutionState::Solved
            );
            assert!(result
                .trace
                .as_ref()
                .unwrap()
                .events
                .iter()
                .any(|event| event.rule == "numeric-evaluation"));
        }

        let symbolic = crate::elaboration::elaborate("N(D(x)(x^2),20)").unwrap();
        let result = execute_elaborated_structure(&mut engine, &symbolic).unwrap();
        assert!(matches!(result.output, ComputationOutput::Held(_)));
        assert!(result.subject().unwrap().print_source().starts_with("N("));
        assert!(result.subject().unwrap().print_source().contains("2*x"));
    }

    #[test]
    fn relation_propagates_no_value_and_rejects_effects() {
        let mut engine = RustEngine::spawn().unwrap();
        let absent = crate::elaboration::elaborate("(Limit(x,0)(1/x))==0").unwrap();
        let result = execute_elaborated_structure(&mut engine, &absent).unwrap();
        assert!(matches!(result.output, ComputationOutput::NoValue(_)));

        let effect = crate::elaboration::elaborate("(Plot(x,x,0,1))==0").unwrap();
        let error = match execute_elaborated_structure(&mut engine, &effect) {
            Ok(_) => panic!("relation should reject effect operand"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("副作用"));
    }
}
