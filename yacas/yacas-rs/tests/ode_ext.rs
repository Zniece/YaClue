use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, source: &str) -> String {
    let expression = parse_expression(env, &format!("{source};"))
        .unwrap_or_else(|error| panic!("parse {source}: {error:?}"))
        .expect("expression");
    let result = eval(env, &expression).unwrap_or_else(|error| panic!("eval {source}: {error:?}"));
    infix_print(env, &result)
}

fn boot() -> Environment {
    let mut env = Environment::new();
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts");
    assert_eq!(
        run(&mut env, &format!("DefaultDirectory(\"{scripts}/\")")),
        "True"
    );
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    env
}

#[test]
fn extended_entry_solves_separable_equations_and_verifies_them() {
    let mut env = boot();
    for equation in ["y'==x*y", "y'==y/x", "y'==(y+1)*Sin(x)"] {
        assert_eq!(
            run(
                &mut env,
                &format!(
                    "[Local(r);r:=ExtendedOdeSolve({equation});{{r[2],Simplify(OdeTest({equation},r[1][1]))}};]"
                )
            ),
            "{Separable,0}",
            "{equation}"
        );
    }
}

#[test]
fn inherited_entry_is_unchanged_and_extended_entry_falls_back() {
    let mut env = boot();
    assert_eq!(run(&mut env, "OdeSolve(y'==x*y)"), "y(1)-x*y(0)");
    assert_eq!(
        run(&mut env, "[Local(r);r:=ExtendedOdeSolve(y'==x);r[2];]"),
        "Upstream"
    );
}

#[test]
fn extended_entry_solves_first_order_linear_equations() {
    let mut env = boot();
    for equation in ["y'+y==x", "y'+2*y==x"] {
        assert_eq!(
            run(
                &mut env,
                &format!(
                    "[Local(r);r:=ExtendedOdeSolve({equation});{{r[2],Simplify(OdeTest({equation},r[1][1]))}};]"
                )
            ),
            "{LinearFirstOrder,0}",
            "{equation}"
        );
    }
}

#[test]
fn bernoulli_extension_keeps_the_equilibrium_branch() {
    let mut env = boot();
    assert_eq!(
        run(
            &mut env,
            "[Local(r);r:=OdeExtSolveBernoulli(y'+y==x*y^2);{r[2],Length(r[1]),Simplify(OdeTest(y'+y==x*y^2,r[1][1])),Simplify(OdeTest(y'+y==x*y^2,r[1][2]))};]"
        ),
        "{Bernoulli,2,0,0}"
    );
}

#[test]
fn exact_extension_returns_a_verified_implicit_solution() {
    let mut env = boot();
    assert_eq!(
        run(
            &mut env,
            "[Local(r);r:=OdeExtSolveExact(2*x*y+3+(x^2+4*y)*y'==0);{r[2],OdeExtVerifyExact(2*x*y+3+(x^2+4*y)*y'==0,r[1][1])};]"
        ),
        "{Exact,True}"
    );
    assert_eq!(
        run(&mut env, "Length(OdeExtSolveExact(y+(x*y)*y'==0))"),
        "0"
    );
}

#[test]
fn homogeneous_extension_keeps_implicit_and_equilibrium_branches() {
    let mut env = boot();
    assert_eq!(
        run(
            &mut env,
            "[Local(r);r:=OdeExtSolveHomogeneous(y'==(x+y)/x);{r[2],Length(r[1]),OdeExtVerifyHomogeneous(y'==(x+y)/x,r[1][1])};]"
        ),
        "{Homogeneous,1,True}"
    );
    assert_eq!(
        run(
            &mut env,
            "[Local(r);r:=OdeExtSolveHomogeneous(y'==(y/x)^2);{r[2],Length(r[1]),OdeExtVerifyHomogeneous(y'==(y/x)^2,r[1][1]),Simplify(OdeTest(y'==(y/x)^2,r[1][2])),Simplify(OdeTest(y'==(y/x)^2,r[1][3]))};]"
        ),
        "{Homogeneous,3,True,0,0}"
    );
    assert_eq!(
        run(&mut env, "Length(OdeExtSolveHomogeneous(y'==x+y))"),
        "0"
    );
}
