use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("非空");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}

#[test]
fn iso() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    for s in [
        "MM(x^2/x)",
        "MM(x^2)",
        "MM(x/x)",
        "MakeMultiNomial(x^2,{x})",
        "MakeMultiNomial(x,{x})",
    ] {
        println!("== [{s}] -> {}", run(&mut env, s));
    }
}
