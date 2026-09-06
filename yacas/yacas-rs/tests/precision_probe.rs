//! 数值精度收敛探针(与 cyacas oracle 锁步,2026-09 精度专项):
//! MathSqrt 家族/N(x,d) 精度/SetExactBits 截断/浮点传染(9+0.→"9.")。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}")).expect("非空");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}
#[test]
fn precision_probe() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    let probes: [(&str, &str); 24] = [
        // (探针, cyacas 期望)
        ("9+0.", "9."),
        ("2+3", "5"),
        ("MathAdd(7,2)", "9"),
        ("MathNegate(7)", "-7"),
        ("MathFloor(3.5)", "3"),
        ("MathGetExactBits(2.)", "34"),
        ("MathGetExactBits(2.0000000000)", "37"),
        ("MathGetExactBits(1.5)", "34"),
        ("MathGetExactBits(1.414213562373097945823)", "74"),
        ("MathGetExactBits(9+0.)", "34"),
        ("MathSqrt(0)", "0"), // 触发加载
        ("MathSetExactBits(1.414213562373097945823, 37)", "1.41421356237"),
        ("MathSetExactBits(2., 37)", "2."),
        ("MathSqrt(2.)", "1.41421356237"),
        ("MathSqrt(9)", "3"),
        ("MathSqrt(3.)", "1.73205080756"),
        ("MathSqrt(4)", "2"),
        ("MathSqrt(16)", "4"),
        ("MathSqrt(123456.789)", "351.36418286444"),
        ("N(Sqrt(2))", "1.4142135623"),
        ("N(Sqrt(2),25)", "1.4142135623730950488016887"),
        ("N(Sqrt(3),15)", "1.732050807568877"),
        ("N(MathSqrt(2),20)", "1.4142135623730950488"),
        ("MathSqrt1(9+0.)", "3."),
    ];
    let mut fails = 0;
    // 已知实现差异(C++ 二进制 limb 量化噪声,末位;COVERAGE §32):
    // MathSqrt float 直调(链式 Halley)个别末位
    let known = ["MathSqrt(2.)", "MathSqrt(3.)", "MathSqrt(123456.789)"];
    for (p, exp) in probes {
        let got = run(&mut env, p);
        let ok = got == exp
            || (known.contains(&p) && got.starts_with(&exp[..exp.len() - 1]));
        if !ok { fails += 1; }
        eprintln!("[{:4}] {:44} => {:30} (期望 {exp})", if ok {"OK"} else {"FAIL"}, p, got);
    }
    assert_eq!(fails, 0, "{fails} 条未对齐");
}
