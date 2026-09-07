//! 端到端语料对拍(第 2 批):微积分/代简/数论/线代/函数式/数值,42 条。
//! probes_p2.txt = 探针,golden_p2.txt = cyacas -pc 逐条输出(cargo run --bin golden -- p2 再生)。
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
fn e2e_payload2() {
    let golden = include_str!("golden_p2.txt");
    let probes = include_str!("probes_p2.txt");
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    let mut golden_outs: Vec<&str> = Vec::new();
    let mut golden_exprs: Vec<&str> = Vec::new();
    for line in golden.lines() {
        if let Some(tab) = line.find('\t') {
            golden_exprs.push(&line[..tab]);
            golden_outs.push(&line[tab + 1..]);
        }
    }
    let mut checked = 0usize;
    let mut fails: Vec<(usize, String)> = Vec::new();
    // 已知实现差异(C++ 二进制 limb 量化噪声,末 1-2 位;位数一致——historical porting notes):
    // 级数链数值函数 Sin/Cos/Tan/Exp(Trig/MathExpTaylor0 循环体)
    let known_diff = |e: &str| {
        e.starts_with("N(Sin(") || e.starts_with("N(Cos(")
            || e.starts_with("N(Tan(") || e.starts_with("N(Exp(")
    };
    let digit_count = |s: &str| s.replace(['.', 'e', 'E', '-'], "").len();
    for line in probes.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some(setup) = line.strip_prefix('>') {
            let _ = run(&mut env, setup);
            continue;
        }
        let actual = run(&mut env, line);
        let expected = golden_outs[checked];
        let known = known_diff(golden_exprs.get(checked).unwrap_or(&""));
        let ok = actual == expected
            || (known && digit_count(&actual) == digit_count(expected)
                && actual.starts_with(&expected[..expected.len().saturating_sub(2)]));
        if !ok {
            // 汇报模式:收集全量分歧,一次看全局再排优先级(不首遇即 panic)
            eprintln!("[DIFF] #{} {} => {:?} (期望 {:?})", checked + 1, golden_exprs.get(checked).unwrap_or(&"? "), actual, expected);
            fails.push((checked + 1, golden_exprs.get(checked).unwrap_or(&"? ").to_string()));
        }
        checked += 1;
    }
    assert_eq!(checked, golden_outs.len(), "应对拍全部 {} 条", golden_outs.len());
    assert!(fails.is_empty(), "e2e 第 2 批 {} 条分歧: {:#?}", fails.len(), fails);
}
