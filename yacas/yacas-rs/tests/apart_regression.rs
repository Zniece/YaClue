use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, source: &str) -> String {
    let tree = parse_expression(env, &format!("{source};"))
        .unwrap()
        .expect("non-empty expression");
    let result = eval(env, &tree).unwrap_or_else(|error| panic!("eval {source}: {error:?}"));
    infix_print(env, &result)
}

#[test]
fn apart_reduces_common_factors_before_partial_fraction_expansion() {
    let mut env = Environment::new();
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    assert_eq!(
        run(&mut env, &format!("DefaultDirectory(\"{scripts}\")")),
        "True"
    );
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    for input in ["(x+1)/(x^2-1)", "(x^2-1)/(x^2-2*x+1)", "1/(x^2-1)"] {
        let apart = run(&mut env, &format!("Apart({input},x)"));
        assert!(!apart.contains("List"), "{input} produced {apart}");
        assert_eq!(
            run(&mut env, &format!("Simplify(({apart})-({input}))")),
            "0",
            "Apart changed the value of {input}: {apart}"
        );
    }
}
