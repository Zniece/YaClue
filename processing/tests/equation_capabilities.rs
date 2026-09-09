use processing::engine::{Engine, Expr, RustEngine};
use processing::equations::{solve, SolveCompleteness, SolveResult, SolveStatus};

struct CapabilityCase {
    category: &'static str,
    name: &'static str,
    equations: &'static [&'static str],
    variables: &'static [&'static str],
    status: SolveStatus,
    solution_count: usize,
    completeness: Option<SolveCompleteness>,
}

const CASES: &[CapabilityCase] = &[
    CapabilityCase {
        category: "polynomial",
        name: "linear",
        equations: &["2*x+3==7"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "polynomial",
        name: "quadratic",
        equations: &["x^2-3*x+2==0"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 2,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "polynomial",
        name: "cubic-complex-roots",
        equations: &["x^3-1==0"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 3,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "polynomial",
        name: "general-quintic",
        equations: &["x^5-x+1==0"],
        variables: &["x"],
        status: SolveStatus::Unresolved,
        solution_count: 0,
        completeness: Some(SolveCompleteness::Unknown),
    },
    CapabilityCase {
        category: "polynomial",
        name: "parameterized-linear",
        equations: &["a*x==1"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Parametric),
    },
    CapabilityCase {
        category: "radical",
        name: "isolated-square-root",
        equations: &["Sqrt(x)==2"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "radical",
        name: "mixed-radical",
        equations: &["Sqrt(x+1)==x-1"],
        variables: &["x"],
        status: SolveStatus::Unresolved,
        solution_count: 0,
        completeness: Some(SolveCompleteness::Unknown),
    },
    CapabilityCase {
        category: "radical",
        name: "impossible-principal-root",
        equations: &["Sqrt(x)==-1"],
        variables: &["x"],
        status: SolveStatus::NoSolution,
        solution_count: 0,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "exponential",
        name: "constant-base",
        equations: &["2^x==8"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "exponential",
        name: "natural-exponential",
        equations: &["Exp(x)==3"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "exponential",
        name: "variable-both-sides",
        equations: &["Exp(x)==x"],
        variables: &["x"],
        status: SolveStatus::Unresolved,
        solution_count: 0,
        completeness: Some(SolveCompleteness::Unknown),
    },
    CapabilityCase {
        category: "logarithmic",
        name: "natural-logarithm",
        equations: &["Ln(x)==2"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "logarithmic",
        name: "parameterized-logarithm",
        equations: &["Ln(x)==a"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Parametric),
    },
    CapabilityCase {
        category: "trigonometric",
        name: "sine-zero-representatives",
        equations: &["Sin(x)==0"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 2,
        completeness: None,
    },
    CapabilityCase {
        category: "trigonometric",
        name: "sine-half-representatives",
        equations: &["Sin(x)==1/2"],
        variables: &["x"],
        status: SolveStatus::Solved,
        solution_count: 2,
        completeness: None,
    },
    CapabilityCase {
        category: "system",
        name: "linear-inferred-variables",
        equations: &["x+y==3", "x-y==1"],
        variables: &[],
        status: SolveStatus::Solved,
        solution_count: 1,
        completeness: Some(SolveCompleteness::Complete),
    },
    CapabilityCase {
        category: "system",
        name: "nonlinear-two-branch",
        equations: &["x^2+y^2==5", "x-y==1"],
        variables: &["x", "y"],
        status: SolveStatus::Solved,
        solution_count: 2,
        completeness: Some(SolveCompleteness::Complete),
    },
];

#[test]
fn algebraic_equation_capability_matrix() {
    let mut engine = RustEngine::spawn().expect("engine boot");
    for case in CASES {
        let result = solve(&mut engine, case.equations, case.variables)
            .unwrap_or_else(|error| panic!("{}/{} failed: {error}", case.category, case.name));
        assert_eq!(
            result.status, case.status,
            "{}/{} status",
            case.category, case.name
        );
        assert_eq!(
            result.solutions.len(),
            case.solution_count,
            "{}/{} solution count",
            case.category,
            case.name
        );
        if let Some(completeness) = case.completeness {
            assert_eq!(
                result.completeness, completeness,
                "{}/{} completeness",
                case.category, case.name
            );
        }
        if result.status == SolveStatus::Solved {
            assert_solutions_satisfy_equations(&mut engine, &result, case.equations);
        }
    }
}

fn assert_solutions_satisfy_equations(
    engine: &mut dyn Engine,
    result: &SolveResult,
    equations: &[&str],
) {
    for solution in &result.solutions {
        for equation in equations {
            let (left, right) = equation.split_once("==").expect("matrix equation uses ==");
            let mut residual = format!("({left})-({right})");
            for assignment in solution {
                residual = format!(
                    "Eval(ApplyPure(\"Subst\",{{{},{},{residual}}}))",
                    assignment.variable, assignment.value
                );
            }
            let checked = engine
                .eval(&format!("Simplify({residual})"))
                .unwrap_or_else(|error| panic!("residual failed for {equation}: {error}"));
            if checked.expr.to_string() != "0" {
                let numeric = engine
                    .eval(&format!("N({residual},30)"))
                    .unwrap_or_else(|error| panic!("numeric residual failed: {error}"));
                let near_zero = match &numeric.expr {
                    Expr::Number(value) => {
                        value.parse::<f64>().is_ok_and(|value| value.abs() < 1e-10)
                    }
                    _ => false,
                };
                assert!(
                    near_zero,
                    "solution {solution:?} does not satisfy {equation}: {}",
                    numeric.expr
                );
            }
        }
    }
}

#[test]
#[ignore = "known defect: representative trigonometric roots are reported as complete"]
fn trigonometric_representatives_are_not_claimed_as_complete() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = solve(&mut engine, &["Sin(x)==0"], &["x"]).unwrap();
    assert_ne!(result.completeness, SolveCompleteness::Complete);
}

#[test]
#[ignore = "known defect: an underdetermined system returns a malformed list assignment"]
fn underdetermined_system_has_a_parametric_contract() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = solve(&mut engine, &["x+y==3"], &["x", "y"]).unwrap();
    assert_eq!(result.status, SolveStatus::Solved);
    assert_eq!(result.completeness, SolveCompleteness::Parametric);
    assert!(result
        .solutions
        .iter()
        .flatten()
        .all(|assignment| result.variables.contains(&assignment.variable)));
}

#[test]
#[ignore = "known defect: an inconsistent system returns Infinity assignments"]
fn inconsistent_system_is_not_a_solution() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = solve(&mut engine, &["x+y==3", "x+y==4"], &["x", "y"]).unwrap();
    assert_eq!(result.status, SolveStatus::NoSolution);
    assert!(result.solutions.is_empty());
}

#[test]
#[ignore = "known defect: a consistent overdetermined system leaks ListNotLongEnough"]
fn consistent_overdetermined_system_is_solved() {
    let mut engine = RustEngine::spawn().unwrap();
    let result = solve(&mut engine, &["x+y==3", "x-y==1", "x==2"], &["x", "y"])
        .expect("overdetermined system should not leak an engine error");
    assert_eq!(result.status, SolveStatus::Solved);
}
