//! Structured processing API for common matrix operations.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression};
use serde::Serialize;

pub const MAX_LINEAR_STRUCTURE_DIMENSION: usize = 16;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinearStructureResult {
    pub rows: usize,
    pub columns: usize,
    pub rref: Vec<Vec<String>>,
    /// One-based column indices, matching mathematical notation and Yacas.
    pub pivot_columns: Vec<usize>,
    pub rank: usize,
    pub nullity: usize,
    pub rows_linearly_independent: bool,
    pub columns_linearly_independent: bool,
    pub null_space_basis: Vec<Vec<String>>,
    pub column_space_basis: Vec<Vec<String>>,
    pub tex: String,
}

/// Compute the exact rational row structure with one elimination pass.
pub fn linear_structure(
    engine: &mut dyn Engine,
    matrix: &str,
) -> Result<LinearStructureResult, EngineError> {
    validate_expression(matrix, "矩阵")?;
    let command = format!(
        "[Local(a); a:={matrix}; \
         Check(IsMatrix(a),\"argument must be a matrix\"); \
         Check(Length(a)>0 And Length(a)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix row limit exceeded\"); \
         Check(Length(a[1])>0 And Length(a[1])<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix column limit exceeded\"); \
         LinearStructure(a);]"
    );
    let evaluated = engine.eval(&command)?;
    let fields = list_items(&evaluated.expr, "LinearStructure result")?;
    if fields.len() != 5 {
        return Err(EngineError::Parse(format!(
            "LinearStructure 返回 {} 个字段，预期 5 个",
            fields.len()
        )));
    }
    let rref = matrix_items(&fields[0], "RREF")?;
    let rows = rref.len();
    let columns = rref.first().map_or(0, Vec::len);
    let rank = usize_item(&fields[1], "秩")?;
    let pivot_columns = usize_list(&fields[2], "主元列")?;
    let null_space_basis = matrix_items(&fields[3], "零空间基")?;
    let column_space_basis = matrix_items(&fields[4], "列空间基")?;
    if rank != pivot_columns.len()
        || rank + null_space_basis.len() != columns
        || column_space_basis.len() != rank
    {
        return Err(EngineError::Parse(
            "LinearStructure 返回的秩、主元或空间维数不一致".into(),
        ));
    }
    Ok(LinearStructureResult {
        rows,
        columns,
        rref,
        pivot_columns,
        rank,
        nullity: null_space_basis.len(),
        rows_linearly_independent: rank == rows,
        columns_linearly_independent: rank == columns,
        null_space_basis,
        column_space_basis,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

fn list_items<'a>(expression: &'a Expr, label: &str) -> Result<&'a [Expr], EngineError> {
    match expression {
        Expr::Call { head, args } if head == "List" => Ok(args),
        _ => Err(EngineError::Parse(format!("{label} 不是列表"))),
    }
}

fn matrix_items(expression: &Expr, label: &str) -> Result<Vec<Vec<String>>, EngineError> {
    let rows = list_items(expression, label)?;
    let matrix = rows
        .iter()
        .map(|row| {
            list_items(row, label).map(|items| items.iter().map(ToString::to_string).collect())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(columns) = matrix.first().map(Vec::len) {
        if matrix.iter().any(|row| row.len() != columns) {
            return Err(EngineError::Parse(format!("{label} 的行长度不一致")));
        }
    }
    Ok(matrix)
}

fn usize_item(expression: &Expr, label: &str) -> Result<usize, EngineError> {
    let Expr::Number(value) = expression else {
        return Err(EngineError::Parse(format!("{label} 不是非负整数")));
    };
    value
        .parse()
        .map_err(|_| EngineError::Parse(format!("{label} 不是非负整数")))
}

fn usize_list(expression: &Expr, label: &str) -> Result<Vec<usize>, EngineError> {
    list_items(expression, label)?
        .iter()
        .map(|item| usize_item(item, label))
        .collect()
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

    #[test]
    fn computes_rank_pivots_and_bases_from_one_reduction() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = linear_structure(&mut engine, "{{1,2,3},{2,4,6},{1,1,1}}").unwrap();
        assert_eq!(result.rows, 3);
        assert_eq!(result.columns, 3);
        assert_eq!(result.rank, 2);
        assert_eq!(result.nullity, 1);
        assert!(!result.rows_linearly_independent);
        assert!(!result.columns_linearly_independent);
        assert_eq!(result.pivot_columns, vec![1, 2]);
        assert_eq!(
            result.rref,
            vec![
                vec!["1", "0", "-1"],
                vec!["0", "1", "2"],
                vec!["0", "0", "0"]
            ]
        );
        assert_eq!(result.null_space_basis, vec![vec!["1", "-2", "1"]]);
        assert_eq!(
            result.column_space_basis,
            vec![vec!["1", "2", "1"], vec!["2", "4", "1"]]
        );
        assert!(!result.tex.is_empty());

        let null_vector = format!("{{{}}}", result.null_space_basis[0].join(","));
        assert_eq!(
            engine
                .eval(&format!(
                    "Simplify({{{{1,2,3}},{{2,4,6}},{{1,1,1}}}}*{null_vector})"
                ))
                .unwrap()
                .expr
                .to_string(),
            "List(0,0,0)"
        );
    }

    #[test]
    fn handles_full_rank_tall_and_wide_rational_matrices() {
        let mut engine = RustEngine::spawn().unwrap();
        let full = linear_structure(&mut engine, "{{1,2},{3,4}}").unwrap();
        assert_eq!((full.rank, full.nullity), (2, 0));
        assert!(full.rows_linearly_independent);
        assert!(full.columns_linearly_independent);
        assert!(full.null_space_basis.is_empty());

        let tall = linear_structure(&mut engine, "{{1,0},{0,1},{1,1}}").unwrap();
        assert_eq!((tall.rows, tall.columns, tall.rank), (3, 2, 2));
        assert!(!tall.rows_linearly_independent);
        assert!(tall.columns_linearly_independent);

        let wide = linear_structure(&mut engine, "{{1,0,2},{0,1,3}}").unwrap();
        assert_eq!(
            (wide.rows, wide.columns, wide.rank, wide.nullity),
            (2, 3, 2, 1)
        );
        assert!(wide.rows_linearly_independent);
        assert!(!wide.columns_linearly_independent);
        assert_eq!(wide.null_space_basis, vec![vec!["-2", "-3", "1"]]);

        let zero = linear_structure(&mut engine, "{{0,0},{0,0}}").unwrap();
        assert_eq!((zero.rank, zero.nullity), (0, 2));
        assert_eq!(zero.pivot_columns, Vec::<usize>::new());
        assert_eq!(zero.null_space_basis, vec![vec!["1", "0"], vec!["0", "1"]]);
        assert!(zero.column_space_basis.is_empty());

        let rational = linear_structure(&mut engine, "{{1/2,1},{1,1}}").unwrap();
        assert_eq!(rational.rref, vec![vec!["1", "0"], vec!["0", "1"]]);
    }

    #[test]
    fn rejects_non_rational_and_oversized_structure_requests() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(linear_structure(&mut engine, "{{1,x},{0,1}}").is_err());
        let oversized = format!("{{{}}}", vec!["1"; 17].join(","));
        assert!(linear_structure(&mut engine, &oversized).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }
}
