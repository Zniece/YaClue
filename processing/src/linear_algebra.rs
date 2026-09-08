//! Structured processing API for common matrix operations.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixOperation {
    Add,
    Multiply,
    Transpose,
    Determinant,
    Inverse,
    Solve,
    Eigenvalues,
}

impl MatrixOperation {
    fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Transpose => "transpose",
            Self::Determinant => "determinant",
            Self::Inverse => "inverse",
            Self::Solve => "solve",
            Self::Eigenvalues => "eigenvalues",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixResult {
    pub operation: MatrixOperation,
    pub output: String,
    pub tex: String,
    /// The engine kept the requested operation unevaluated.
    pub unresolved: bool,
}

/// Apply a matrix operation. `right` is required for addition,
/// multiplication and linear-system solving, and rejected otherwise.
pub fn compute(
    engine: &mut dyn Engine,
    left: &str,
    operation: MatrixOperation,
    right: Option<&str>,
) -> Result<MatrixResult, EngineError> {
    validate_expression(left, "矩阵")?;
    if let Some(right) = right {
        validate_expression(right, "右操作数")?;
    }

    let binary = matches!(
        operation,
        MatrixOperation::Add | MatrixOperation::Multiply | MatrixOperation::Solve
    );
    if binary != right.is_some() {
        return Err(EngineError::InvalidInput(if binary {
            format!("{} 需要右操作数", operation.name())
        } else {
            format!("{} 不接受右操作数", operation.name())
        }));
    }

    let command = checked_command(left, operation, right);
    let result = engine.eval(&command)?;
    let unresolved = unresolved(&result.expr, operation);
    Ok(MatrixResult {
        operation,
        output: result.expr.to_string(),
        tex: strip_tex_delimiters(&result.tex),
        unresolved,
    })
}

fn checked_command(left: &str, operation: MatrixOperation, right: Option<&str>) -> String {
    let matrix_check = "Check(IsMatrix(a),\"left operand must be a matrix\")";
    let square_check = "Check(IsSquareMatrix(a),\"matrix must be square\")";
    let singular_check = "Check(Not(IsZero(Determinant(a))),\"matrix must be nonsingular\")";
    let body = match operation {
        MatrixOperation::Add => format!(
            "b:={}; Check(IsMatrix(b),\"right operand must be a matrix\"); \
             Check(Dimensions(a)=Dimensions(b),\"matrix dimensions must match\"); a+b",
            right.unwrap()
        ),
        MatrixOperation::Multiply => format!(
            "b:={}; Check(IsMatrix(b),\"right operand must be a matrix\"); \
             Check(Length(a[1])=Length(b),\"matrix dimensions are incompatible\"); a*b",
            right.unwrap()
        ),
        MatrixOperation::Transpose => "Transpose(a)".into(),
        MatrixOperation::Determinant => format!("{square_check}; Determinant(a)"),
        MatrixOperation::Inverse => {
            format!("{square_check}; {singular_check}; Inverse(a)")
        }
        MatrixOperation::Solve => format!(
            "b:={}; Check(IsVector(b),\"right operand must be a vector\"); {square_check}; \
             Check(Length(a)=Length(b),\"matrix and vector dimensions must match\"); \
             {singular_check}; MatrixSolve(a,b)",
            right.unwrap()
        ),
        MatrixOperation::Eigenvalues => format!("{square_check}; EigenValues(a)"),
    };
    format!("[Local(a,b); a:={left}; {matrix_check}; {body};]")
}

fn unresolved(expr: &Expr, operation: MatrixOperation) -> bool {
    let expected = match operation {
        MatrixOperation::Transpose => "Transpose",
        MatrixOperation::Determinant => "Determinant",
        MatrixOperation::Inverse => "Inverse",
        MatrixOperation::Solve => "MatrixSolve",
        MatrixOperation::Eigenvalues => "EigenValues",
        MatrixOperation::Add | MatrixOperation::Multiply => return false,
    };
    matches!(expr, Expr::Call { head, .. } if head == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RustEngine;

    #[test]
    fn computes_basic_matrix_operations() {
        let mut engine = RustEngine::spawn().unwrap();
        for (operation, left, right, expected) in [
            (
                MatrixOperation::Add,
                "{{1,2},{3,4}}",
                Some("{{5,6},{7,8}}"),
                "List(List(6,8),List(10,12))",
            ),
            (
                MatrixOperation::Multiply,
                "{{1,2},{3,4}}",
                Some("{{5,6},{7,8}}"),
                "List(List(19,22),List(43,50))",
            ),
            (
                MatrixOperation::Transpose,
                "{{1,2,3},{4,5,6}}",
                None,
                "List(List(1,4),List(2,5),List(3,6))",
            ),
            (MatrixOperation::Determinant, "{{1,2},{3,4}}", None, "-2"),
            (
                MatrixOperation::Solve,
                "{{2,1},{1,-1}}",
                Some("{5,1}"),
                "List(2,1)",
            ),
            (
                MatrixOperation::Eigenvalues,
                "{{2,0},{0,3}}",
                None,
                "List(2,3)",
            ),
        ] {
            let result = compute(&mut engine, left, operation, right).unwrap();
            assert_eq!(result.output, expected);
            assert!(!result.tex.is_empty());
            assert!(!result.unresolved);
        }
    }

    #[test]
    fn computes_exact_inverse_and_symbolic_eigenvalues() {
        let mut engine = RustEngine::spawn().unwrap();
        let inverse =
            compute(&mut engine, "{{1,2},{3,4}}", MatrixOperation::Inverse, None).unwrap();
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify({}*{{{{1,2}},{{3,4}}}}-Identity(2))",
                    inverse.output
                ))
                .unwrap()
                .expr
                .to_string(),
            "List(List(0,0),List(0,0))"
        );

        let eigenvalues = compute(
            &mut engine,
            "{{1,2},{3,4}}",
            MatrixOperation::Eigenvalues,
            None,
        )
        .unwrap();
        assert!(eigenvalues.output.contains("Sqrt(33)"));
    }

    #[test]
    fn rejects_bad_dimensions_singular_matrices_and_bad_requests() {
        let mut engine = RustEngine::spawn().unwrap();
        for result in [
            compute(
                &mut engine,
                "{{1,2,3},{4,5,6}}",
                MatrixOperation::Determinant,
                None,
            ),
            compute(&mut engine, "{{1,2},{2,4}}", MatrixOperation::Inverse, None),
            compute(
                &mut engine,
                "{{1,2},{2,4}}",
                MatrixOperation::Solve,
                Some("{3,6}"),
            ),
            compute(
                &mut engine,
                "{{1,2},{3,4}}",
                MatrixOperation::Multiply,
                Some("{{1,2}}"),
            ),
            compute(
                &mut engine,
                "{{1,2},{3,4}}",
                MatrixOperation::Solve,
                Some("{1}"),
            ),
            compute(&mut engine, "{{1,2},{3,4}}", MatrixOperation::Add, None),
        ] {
            assert!(result.is_err());
        }
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }

    #[test]
    fn rejects_non_matrices_and_injected_input() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(compute(&mut engine, "x", MatrixOperation::Transpose, None).is_err());
        assert!(compute(
            &mut engine,
            "{{1}});Echo(1);({{1}}",
            MatrixOperation::Transpose,
            None,
        )
        .is_err());
    }
}
