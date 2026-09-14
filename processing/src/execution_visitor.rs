//! Bounded inside-out traversal of elaborated mathematical objects.
//!
//! Node/domain algorithms remain in their owning modules; this module owns
//! traversal order, registered application dispatch and trace finalization.

use crate::arithmetic::{
    execute_container, execute_function_application, ArithmeticOperation,
    ArithmeticOperationExecutor, ArithmeticRequest,
};
use crate::engine::{Engine, EngineError};
use crate::protocol::ResolutionState;
use crate::semantic_core::{
    BinarySemanticOperation, Computation, ComputationOutput, SemanticInterpretation,
    UnarySemanticOperation,
};

pub fn execute_elaborated_structure(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    execute_elaborated_structure_in_context(engine, expression)
}

pub fn execute_elaborated_structure_with_context(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
    context: crate::semantic_core::ComputationContext,
) -> Result<Computation, EngineError> {
    crate::semantic_core::with_computation_context(context, || {
        execute_elaborated_structure_in_context(engine, expression)
    })
}

fn execute_elaborated_structure_in_context(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    let mut computation = execute_elaborated_node(engine, expression)?;
    if matches!(
        expression.form,
        crate::elaboration::MathematicalForm::Structural { .. }
    ) && computation
        .subject()
        .is_some_and(|result| result.print_source() != expression.object.print_source())
    {
        if let Some(event) = computation
            .trace
            .as_mut()
            .and_then(|trace| trace.events.last_mut())
            .filter(|event| {
                event.class == crate::semantic_core::RuleEventClass::InternalExecution
                    && event.output.object == expression.object.id
            })
        {
            event.class = crate::semantic_core::RuleEventClass::EquivalentTransformation;
            if let Some(presentation) = event.presentation.as_mut() {
                presentation.explanation = "化简这个子表达式。".into();
            }
        }
    }
    if let Some(trace) = computation.trace.as_mut() {
        for event in &mut trace.events {
            if event.class == crate::semantic_core::RuleEventClass::EquivalentTransformation
                && event.transformation.is_none()
            {
                event.transformation = Some(crate::semantic_core::TransformationContext {
                    root_before: crate::semantic_core::ObjectReference {
                        object: expression.object.id,
                        revision: event.input.revision,
                        focus: None,
                    },
                    root_after: crate::semantic_core::ObjectReference {
                        object: expression.object.id,
                        revision: event.output.revision,
                        focus: None,
                    },
                    focus: crate::semantic_core::ExpressionPath::root(),
                });
            }
        }
        trace.apply_mode(crate::semantic_core::current_computation_context().trace_mode);
    }
    Ok(computation)
}

fn execute_elaborated_node(
    engine: &mut dyn Engine,
    expression: &crate::elaboration::ElaboratedObject,
) -> Result<Computation, EngineError> {
    if let crate::elaboration::MathematicalForm::Application { head } = &expression.form {
        if let Some(lowered) = crate::lowering::try_lower_application(
            engine,
            &expression.object,
            crate::semantic_core::current_computation_context().trace_mode,
        )? {
            return Ok(lowered);
        }
        if let Some(descriptor) = crate::semantic_core::operator_descriptor(head) {
            return (descriptor.execution_handler)(
                engine,
                expression,
                head,
                execute_elaborated_structure,
            );
        }
        return execute_function_application(
            engine,
            execute_elaborated_structure,
            expression,
            head,
        );
    }
    if matches!(
        expression.form,
        crate::elaboration::MathematicalForm::Relation { .. }
            | crate::elaboration::MathematicalForm::Collection
    ) {
        return execute_container(expression, engine, execute_elaborated_structure);
    }
    if let crate::elaboration::MathematicalForm::EffectApplication { head } = &expression.form {
        let descriptor = crate::semantic_core::operator_descriptor(head)
            .ok_or_else(|| EngineError::InvalidInput(format!("未登记的效果运算符: {head}")))?;
        return (descriptor.execution_handler)(
            engine,
            expression,
            head,
            execute_elaborated_structure,
        );
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
