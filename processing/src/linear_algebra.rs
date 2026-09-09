//! Structured processing API for common matrix operations.

use crate::engine::{Engine, EngineError, Expr};
use crate::input::{strip_tex_delimiters, validate_expression};
use crate::steps::{render_events, Step, StepEvent, StepImportance, StepVerbosity};
use serde::Serialize;

pub const MAX_LINEAR_STRUCTURE_DIMENSION: usize = 16;
pub const MAX_EIGENVALUES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlduResult {
    pub dimension: usize,
    pub permutation: Vec<Vec<String>>,
    pub lower: Vec<Vec<String>>,
    pub diagonal: Vec<Vec<String>>,
    pub upper: Vec<Vec<String>>,
    /// Whether the engine verified `P*A = L*D*U` in the same evaluation.
    pub verified: bool,
    pub tex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CholeskyResult {
    pub dimension: usize,
    /// Upper-triangular factor satisfying `A = Transpose(R)*R`.
    pub upper: Vec<Vec<String>>,
    pub verified: bool,
    pub tex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GramSchmidtResult {
    pub vector_count: usize,
    pub vector_dimension: usize,
    pub basis: Vec<Vec<String>>,
    pub normalized: bool,
    /// Whether pairwise orthogonality (and unit norms when requested) was verified.
    pub verified: bool,
    pub tex: String,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowOperationKind {
    Swap,
    Scale,
    AddMultiple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowOperation {
    pub kind: RowOperationKind,
    /// One-based row index, matching mathematical notation and Yacas.
    pub target_row: usize,
    pub source_row: Option<usize>,
    pub factor: String,
    pub matrix: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinearStructureStepResult {
    pub result: LinearStructureResult,
    pub operations: Vec<RowOperation>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EigenSpace {
    pub eigenvalue: String,
    pub basis: Vec<Vec<String>>,
    pub geometric_multiplicity: usize,
    pub is_eigenvalue: bool,
    /// The standard script verifies `A*v = lambda*v` before returning.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EigenSpaceResult {
    pub dimension: usize,
    pub spaces: Vec<EigenSpace>,
    pub tex: String,
}

/// Compute a pivoted exact `P*A = L*D*U` decomposition.
pub fn pldu(engine: &mut dyn Engine, matrix: &str) -> Result<PlduResult, EngineError> {
    validate_expression(matrix, "矩阵")?;
    let command = format!(
        "[Local(a,p,l,d,u,ok); a:={matrix}; \
         Check(IsSquareMatrix(a),\"argument must be a square matrix\"); \
         Check(Length(a)>0 And Length(a)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix dimension limit exceeded\"); \
         {{p,l,d,u}}:=PLDU(a); ok:=(p*a-l*d*u)=ZeroMatrix(Length(a)); \
         {{Length(a),p,l,d,u,ok}};]"
    );
    let evaluated = engine.eval(&command)?;
    let fields = exact_fields(&evaluated.expr, "PLDU result", 6)?;
    let dimension = usize_item(&fields[0], "矩阵维数")?;
    Ok(PlduResult {
        dimension,
        permutation: square_matrix(&fields[1], "置换矩阵", dimension)?,
        lower: square_matrix(&fields[2], "下三角矩阵", dimension)?,
        diagonal: square_matrix(&fields[3], "对角矩阵", dimension)?,
        upper: square_matrix(&fields[4], "上三角矩阵", dimension)?,
        verified: bool_item(&fields[5], "PLDU 证书")?,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

/// Compute an exact upper Cholesky factor after checking symmetry.
pub fn cholesky(engine: &mut dyn Engine, matrix: &str) -> Result<CholeskyResult, EngineError> {
    validate_expression(matrix, "矩阵")?;
    let command = format!(
        "[Local(a,r,ok); a:={matrix}; \
         Check(IsSquareMatrix(a),\"argument must be a square matrix\"); \
         Check(Length(a)>0 And Length(a)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix dimension limit exceeded\"); \
         Check(a=Transpose(a),\"matrix must be symmetric\"); \
         r:=Cholesky(a); ok:=(Transpose(r)*r-a)=ZeroMatrix(Length(a)); \
         {{Length(a),r,ok}};]"
    );
    let evaluated = engine.eval(&command)?;
    let fields = exact_fields(&evaluated.expr, "Cholesky result", 3)?;
    let dimension = usize_item(&fields[0], "矩阵维数")?;
    Ok(CholeskyResult {
        dimension,
        upper: square_matrix(&fields[1], "Cholesky 因子", dimension)?,
        verified: bool_item(&fields[2], "Cholesky 证书")?,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

/// Orthogonalize a linearly independent vector family. The vectors are
/// normalized when `normalized` is true.
pub fn gram_schmidt(
    engine: &mut dyn Engine,
    vectors: &str,
    normalized: bool,
) -> Result<GramSchmidtResult, EngineError> {
    validate_expression(vectors, "向量组")?;
    let algorithm = if normalized {
        "OrthonormalBasis(w)"
    } else {
        "OrthogonalBasis(w)"
    };
    let unit_check = if normalized {
        "For(i:=1,i<=Length(b),i++) If(Not(Simplify(InProduct(b[i],b[i]))=1),ok:=False);"
    } else {
        ""
    };
    let command = format!(
        "[Local(w,s,b,ok,i,j); w:={vectors}; \
         Check(IsMatrix(w),\"vectors must have equal dimensions\"); \
         Check(Length(w)>0 And Length(w)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"vector count limit exceeded\"); \
         Check(Length(w[1])>0 And Length(w[1])<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"vector dimension limit exceeded\"); \
         s:=LinearStructure(w); Check(s[2]=Length(w),\"vectors must be linearly independent\"); \
         b:={algorithm}; \
         ok:=True; For(i:=1,i<Length(b),i++) For(j:=i+1,j<=Length(b),j++) \
              If(Not(InProduct(b[i],b[j])=0),ok:=False); \
         {unit_check} \
         {{Length(w),Length(w[1]),b,ok}};]"
    );
    let evaluated = engine.eval(&command)?;
    let fields = exact_fields(&evaluated.expr, "Gram-Schmidt result", 4)?;
    let vector_count = usize_item(&fields[0], "向量数量")?;
    let vector_dimension = usize_item(&fields[1], "向量维数")?;
    let basis = matrix_items(&fields[2], "正交基")?;
    if basis.len() != vector_count || basis.iter().any(|v| v.len() != vector_dimension) {
        return Err(EngineError::Parse("Gram-Schmidt 返回的基维数不一致".into()));
    }
    Ok(GramSchmidtResult {
        vector_count,
        vector_dimension,
        basis,
        normalized,
        verified: bool_item(&fields[3], "Gram-Schmidt 证书")?,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

fn exact_fields<'a>(
    expression: &'a Expr,
    label: &str,
    count: usize,
) -> Result<&'a [Expr], EngineError> {
    let fields = list_items(expression, label)?;
    if fields.len() != count {
        return Err(EngineError::Parse(format!(
            "{label} 返回 {} 个字段，预期 {count} 个",
            fields.len()
        )));
    }
    Ok(fields)
}

fn square_matrix(
    expression: &Expr,
    label: &str,
    dimension: usize,
) -> Result<Vec<Vec<String>>, EngineError> {
    let matrix = matrix_items(expression, label)?;
    if matrix.len() != dimension || matrix.iter().any(|row| row.len() != dimension) {
        return Err(EngineError::Parse(format!("{label} 维数与输入矩阵不一致")));
    }
    Ok(matrix)
}

fn bool_item(expression: &Expr, label: &str) -> Result<bool, EngineError> {
    match expression.to_string().as_str() {
        "True" => Ok(true),
        "False" => Ok(false),
        _ => Err(EngineError::Parse(format!("{label} 不是布尔值"))),
    }
}

/// Compute exact eigenspace bases for a rational square matrix. When
/// `eigenvalues` is empty, the engine first computes the eigenvalues.
pub fn eigen_spaces(
    engine: &mut dyn Engine,
    matrix: &str,
    eigenvalues: &[&str],
) -> Result<EigenSpaceResult, EngineError> {
    validate_expression(matrix, "矩阵")?;
    if eigenvalues.len() > MAX_EIGENVALUES {
        return Err(EngineError::InvalidInput(format!(
            "特征值数量不能超过 {MAX_EIGENVALUES}"
        )));
    }
    for eigenvalue in eigenvalues {
        validate_expression(eigenvalue, "特征值")?;
    }
    let values = if eigenvalues.is_empty() {
        "EigenValues(a)".to_string()
    } else {
        format!("{{{}}}", eigenvalues.join(","))
    };
    let command = format!(
        "[Local(a,values); a:={matrix}; \
         Check(IsSquareMatrix(a),\"argument must be a square matrix\"); \
         Check(Length(a)>0 And Length(a)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix dimension limit exceeded\"); \
         values:={values}; {{Length(a),EigenSpaces(a,values)}};]"
    );
    let evaluated = engine.eval(&command)?;
    let fields = list_items(&evaluated.expr, "EigenSpaces result")?;
    if fields.len() != 2 {
        return Err(EngineError::Parse(format!(
            "EigenSpaces 返回 {} 个字段，预期 2 个",
            fields.len()
        )));
    }
    let dimension = usize_item(&fields[0], "矩阵维数")?;
    let groups = list_items(&fields[1], "特征空间列表")?;
    let spaces = groups
        .iter()
        .map(|group| parse_eigen_space(group, dimension))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EigenSpaceResult {
        dimension,
        spaces,
        tex: strip_tex_delimiters(&evaluated.tex),
    })
}

fn parse_eigen_space(expression: &Expr, dimension: usize) -> Result<EigenSpace, EngineError> {
    let fields = list_items(expression, "特征空间")?;
    if fields.len() != 2 {
        return Err(EngineError::Parse("特征空间必须包含特征值和基".into()));
    }
    let basis = matrix_items(&fields[1], "特征空间基")?;
    if basis.iter().any(|vector| vector.len() != dimension) {
        return Err(EngineError::Parse("特征向量维度与矩阵不一致".into()));
    }
    Ok(EigenSpace {
        eigenvalue: fields[0].to_string(),
        geometric_multiplicity: basis.len(),
        is_eigenvalue: !basis.is_empty(),
        basis,
        verified: true,
    })
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
    let fields = exact_fields(&evaluated.expr, "LinearStructure result", 5)?;
    parse_linear_structure(fields, strip_tex_delimiters(&evaluated.tex))
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
    validate_expression(matrix, "矩阵")?;
    let command = format!(
        "[Local(a); a:={matrix}; \
         Check(IsMatrix(a),\"argument must be a matrix\"); \
         Check(Length(a)>0 And Length(a)<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix row limit exceeded\"); \
         Check(Length(a[1])>0 And Length(a[1])<={MAX_LINEAR_STRUCTURE_DIMENSION},\
               \"matrix column limit exceeded\"); \
         LinearStructureDetailed(a);]"
    );
    let evaluated = engine.eval_expr(&command)?;
    let fields = exact_fields(&evaluated, "LinearStructureDetailed result", 6)?;
    let mut result = parse_linear_structure(&fields[..5], String::new())?;
    let operations = list_items(&fields[5], "行操作事件")?
        .iter()
        .map(parse_row_operation)
        .collect::<Result<Vec<_>, _>>()?;
    let mut events = vec![StepEvent::new(
        "row-reduction-start",
        matrix,
        "从原矩阵开始进行高斯–若尔当消元。",
        StepImportance::Routine,
    )];
    for operation in &operations {
        let (rule, why, importance) = match operation.kind {
            RowOperationKind::Swap => (
                "row-swap",
                format!(
                    "交换第 {} 行与第 {} 行，使当前列获得非零主元。",
                    operation.target_row,
                    operation
                        .source_row
                        .expect("swap operations have a source row")
                ),
                StepImportance::Key,
            ),
            RowOperationKind::Scale => (
                "row-scale",
                format!(
                    "第 {} 行乘以 {}，将主元化为 1。",
                    operation.target_row, operation.factor
                ),
                StepImportance::Normal,
            ),
            RowOperationKind::AddMultiple => (
                "row-eliminate",
                format!(
                    "第 {} 行加上第 {} 行的 {} 倍，消去当前列元素。",
                    operation.target_row,
                    operation
                        .source_row
                        .expect("elimination operations have a source row"),
                    operation.factor
                ),
                StepImportance::Normal,
            ),
        };
        events.push(StepEvent::new(
            rule,
            &matrix_expression(&operation.matrix),
            &why,
            importance,
        ));
    }
    events.push(StepEvent::new(
        "row-reduction-result",
        &matrix_expression(&result.rref),
        &format!(
            "得到行最简形；秩为 {}，主元列为 {:?}。",
            result.rank, result.pivot_columns
        ),
        StepImportance::Key,
    ));
    let steps = render_events(engine, events, verbosity)?;
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

fn parse_linear_structure(
    fields: &[Expr],
    tex: String,
) -> Result<LinearStructureResult, EngineError> {
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
        tex,
    })
}

fn parse_row_operation(expression: &Expr) -> Result<RowOperation, EngineError> {
    let fields = exact_fields(expression, "行操作事件", 5)?;
    let kind = match fields[0].to_string().trim_matches('"') {
        "swap" => RowOperationKind::Swap,
        "scale" => RowOperationKind::Scale,
        "eliminate" => RowOperationKind::AddMultiple,
        other => return Err(EngineError::Parse(format!("未知行操作类型: {other}"))),
    };
    let target_row = usize_item(&fields[1], "目标行")?;
    let source = usize_item(&fields[2], "来源行")?;
    let source_row = (kind != RowOperationKind::Scale).then_some(source);
    Ok(RowOperation {
        kind,
        target_row,
        source_row,
        factor: fields[3].to_string(),
        matrix: matrix_items(&fields[4], "行操作后的矩阵")?,
    })
}

fn matrix_expression(matrix: &[Vec<String>]) -> String {
    format!(
        "{{{}}}",
        matrix
            .iter()
            .map(|row| format!("{{{}}}", row.join(",")))
            .collect::<Vec<_>>()
            .join(",")
    )
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
    use crate::test_support::CountingEngine;

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

    #[test]
    fn computes_grouped_verified_eigenspaces() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = eigen_spaces(&mut engine, "{{2,0,0},{0,2,0},{0,0,3}}", &[]).unwrap();
        assert_eq!(result.dimension, 3);
        assert_eq!(result.spaces.len(), 2);
        assert_eq!(result.spaces[0].eigenvalue, "2");
        assert_eq!(result.spaces[0].geometric_multiplicity, 2);
        assert_eq!(
            result.spaces[0].basis,
            vec![vec!["1", "0", "0"], vec!["0", "1", "0"]]
        );
        assert_eq!(result.spaces[1].eigenvalue, "3");
        assert_eq!(result.spaces[1].basis, vec![vec!["0", "0", "1"]]);
        assert!(result.spaces.iter().all(|space| space.verified));
        assert!(!result.tex.is_empty());
    }

    #[test]
    fn reports_requested_values_that_are_not_eigenvalues() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = eigen_spaces(&mut engine, "{{2,0},{0,3}}", &["2", "4", "2"]).unwrap();
        assert_eq!(result.spaces.len(), 2);
        assert!(result.spaces[0].is_eigenvalue);
        assert!(!result.spaces[1].is_eigenvalue);
        assert!(result.spaces[1].basis.is_empty());
    }

    #[test]
    fn rejects_eigenspaces_outside_the_exact_rational_contract() {
        let mut engine = RustEngine::spawn().unwrap();
        assert!(eigen_spaces(&mut engine, "{{1,2},{3,4}}", &[]).is_err());
        assert!(eigen_spaces(&mut engine, "{{x,0},{0,1}}", &["1"]).is_err());
        assert!(eigen_spaces(&mut engine, "{{1,0},{0,1}}", &["Sqrt(2)"]).is_err());
        assert_eq!(engine.eval("2+3").unwrap().expr.to_string(), "5");
    }

    #[test]
    fn returns_verified_pivoted_pldu_factors() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = pldu(&mut engine, "{{0,1},{1,2}}").unwrap();
        assert_eq!(result.dimension, 2);
        assert_eq!(result.permutation, vec![vec!["0", "1"], vec!["1", "0"]]);
        assert!(result.verified);
        assert!(!result.tex.is_empty());
    }

    #[test]
    fn returns_verified_cholesky_factor_and_rejects_bad_inputs() {
        let mut engine = RustEngine::spawn().unwrap();
        let result = cholesky(&mut engine, "{{4,2},{2,3}}").unwrap();
        assert_eq!(result.dimension, 2);
        assert_eq!(result.upper, vec![vec!["2", "1"], vec!["0", "Sqrt(2)"]]);
        assert!(result.verified);
        assert!(cholesky(&mut engine, "{{1,2},{0,1}}").is_err());
        assert!(cholesky(&mut engine, "{{1,2},{2,1}}").is_err());
    }

    #[test]
    fn returns_verified_orthogonal_and_orthonormal_bases() {
        let mut engine = RustEngine::spawn().unwrap();
        let orthogonal = gram_schmidt(&mut engine, "{{1,1,0},{2,0,1},{2,2,1}}", false).unwrap();
        assert_eq!(
            (orthogonal.vector_count, orthogonal.vector_dimension),
            (3, 3)
        );
        assert!(!orthogonal.normalized);
        assert!(orthogonal.verified);

        let orthonormal = gram_schmidt(&mut engine, "{{1,0},{0,1}}", true).unwrap();
        assert!(orthonormal.normalized);
        assert!(orthonormal.verified);
        assert!(gram_schmidt(&mut engine, "{{1,0},{2,0}}", false).is_err());
        assert!(gram_schmidt(&mut engine, "{{1,0},{1}}", false).is_err());
    }

    #[test]
    fn row_reduction_steps_reuse_detailed_elimination_events() {
        let mut engine = CountingEngine::spawn();
        let detailed = linear_structure_steps(&mut engine, "{{0,2},{1,1}}").unwrap();
        assert_eq!(detailed.result.rref, vec![vec!["1", "0"], vec!["0", "1"]]);
        assert!(detailed
            .operations
            .iter()
            .any(|operation| operation.kind == RowOperationKind::Swap));
        assert!(detailed
            .operations
            .iter()
            .any(|operation| operation.kind == RowOperationKind::Scale));
        assert!(detailed
            .operations
            .iter()
            .any(|operation| operation.kind == RowOperationKind::AddMultiple));
        assert_eq!(engine.eval_calls, 1);
        assert_eq!(engine.batch_sizes, [detailed.steps.len()]);

        engine.reset_counts();
        let concise = linear_structure_steps_with_verbosity(
            &mut engine,
            "{{0,2},{1,1}}",
            StepVerbosity::Concise,
        )
        .unwrap();
        assert!(concise.steps.len() < detailed.steps.len());
        assert_eq!(engine.eval_calls, 1);
        assert_eq!(engine.batch_sizes, [concise.steps.len()]);
    }

    #[test]
    fn detailed_row_reduction_matches_the_fast_structure_path() {
        let mut engine = RustEngine::spawn().unwrap();
        for matrix in ["{{0,2},{1,1}}", "{{1,2,3},{2,4,6}}", "{{1,2},{3,4},{5,6}}"] {
            let mut fast = linear_structure(&mut engine, matrix).unwrap();
            let mut detailed = linear_structure_steps(&mut engine, matrix).unwrap().result;
            fast.tex.clear();
            detailed.tex.clear();
            assert_eq!(detailed, fast, "structure mismatch for {matrix}");
        }
    }
}
