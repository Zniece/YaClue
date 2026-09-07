use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("nonempty expression");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

#[test]
fn scientific_fraction_floor_ceil_and_radian_reduction() {
    let mut env = Environment::new();
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    assert_eq!(
        run(&mut env, &format!("DefaultDirectory(\"{scripts}\")")),
        "True"
    );
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");

    assert_eq!(run(&mut env, "MathFloor(-0.9370247274e-1)"), "-1");
    assert_eq!(run(&mut env, "MathCeil(-0.9370247274e-1)"), "0");

    let reduced = run(&mut env, "TruncRadian(-0.58875)")
        .parse::<f64>()
        .expect("TruncRadian should produce a numeric result");
    assert!(
        (0.0..std::f64::consts::TAU).contains(&reduced),
        "unexpected angle: {reduced}"
    );
    assert!(
        (reduced - (std::f64::consts::TAU - 0.58875)).abs() < 1e-9,
        "unexpected angle: {reduced}"
    );
}
