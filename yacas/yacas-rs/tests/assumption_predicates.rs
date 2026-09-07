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
fn known_predicates_propagate_only_safe_symbolic_facts() {
    let mut env = boot();
    assert_eq!(run(&mut env, "Assume(x,Positive)"), "True");
    assert_eq!(run(&mut env, "Assume(n,Integer)"), "True");
    assert_eq!(run(&mut env, "Assume(z,Real)"), "True");
    assert_eq!(run(&mut env, "Assume(k,Integer)"), "True");
    assert_eq!(run(&mut env, "IsKnownReal(z^k)"), "False");
    assert_eq!(run(&mut env, "Assume(z,NonZero)"), "True");
    assert_eq!(run(&mut env, "IsKnownReal(z^k)"), "True");
    for expression in [
        "IsKnownReal(x)",
        "IsKnownReal(x^n)",
        "IsKnownInteger(n+2)",
        "IsKnownPositive(Exp(x))",
        "IsKnownPositive(x^2)",
        "IsKnownPositive(2*x)",
        "IsKnownPositive(x+2)",
        "IsKnownPositive(Abs(x))",
        "IsKnownReal(Ln(x))",
        "IsKnownNonZero(-3*x)",
    ] {
        assert_eq!(run(&mut env, expression), "True", "{expression}");
    }

    for expression in [
        "IsKnownInteger(x)",
        "IsKnownPositive(n)",
        "IsKnownReal(y)",
        "IsKnownNonZero(y)",
    ] {
        assert_eq!(run(&mut env, expression), "False", "{expression}");
    }

    // Value predicates retain their historical meaning.
    assert_eq!(run(&mut env, "IsInteger(n)"), "False");
}
