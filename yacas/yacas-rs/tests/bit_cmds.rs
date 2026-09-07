//! 位级数值命令对拍(照 cyacas mathcommands3.cpp + elemfuncs.ys 依赖)。
//! DigitsToBits/BitsToDigits 精确(曾 ×4 近似 → 60/12;应 50/15);
//! MathBitCount/GetExactBits 支持浮点/大数(曾 i64 截断 + 浮点 InvalidArg)。
//! 期望全部来自 cyacas oracle。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap().expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
#[test]
fn bit_cmds() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let probes = [
        ("DigitsToBits(15,10)", "50"),
        ("BitsToDigits(50,10)", "15"),
        ("MathBitCount(2)", "2"),
        ("MathBitCount(2.)", "2"),
        ("MathBitCount(2.5)", "2"),
        ("MathBitCount(2^100)", "101"),
        ("MathBitCount(0)", "0"),
        ("MathBitCount(-5)", "3"),
        ("MathBitCount(0.5)", "0"),
        ("MathBitCount(1e100)", "333"),
        ("MathBitCount(1e1000)", "3322"),
        ("MathGetExactBits(2)", "2"),
        ("MathGetExactBits(2.)", "34"),
        ("MathGetExactBits(2^64)", "65"),
        ("Mod(-7,3)", "2"),
        ("Mod(-9223372036854775809,10)", "1"),
        ("Mod(7,-3)", "Mod(7,-3)"),
        ("ShiftLeft(9223372036854775808,1)", "18446744073709551616"),
        ("ShiftRight(9223372036854775808,1)", "4611686018427387904"),
        ("ShiftLeft(-9223372036854775809,1)", "-18446744073709551618"),
        ("ShiftRight(-9223372036854775809,1)", "-4611686018427387904"),
        ("ShiftLeft(1,100)", "1267650600228229401496703205376"),
        ("ShiftRight(-5,100)", "0"),
        ("MathGcd(-9223372036854775808,0)", "9223372036854775808"),
    ];
    for (p, exp) in probes {
        assert_eq!(run(&mut e, p), exp, "probe {p}");
    }
}
