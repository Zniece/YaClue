//! 层 4 验收:解析 + FullForm 打印与 cyacas golden_l4.txt 逐字节对拍。
//!
//! golden_l4.txt 记录 = `FullForm(<expr>)` + TAB + 打印块(嵌套子列表含真实换行+缩进),
//! 因此读取用状态机:以 `FullForm(` 开头且含 TAB 的行为记录头,其余行续接上一记录的打印块。

use yacas_rs::env::Environment;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::full_form;

#[test]
fn golden_fullform_l4() {
    let golden = include_str!("golden_l4.txt");
    let mut expr: Option<String> = None;
    let mut expected = String::new();
    let mut checked = 0usize;
    for line in golden.lines() {
        if let Some(tab) = line.find('\t') {
            if line.starts_with("FullForm(") {
                if let Some(e) = expr.take() {
                    check(&e, &expected);
                    checked += 1;
                    expected.clear();
                }
                expr = Some(line[..tab].to_string());
                expected.push_str(&line[tab + 1..]);
                continue;
            }
        }
        expected.push('\n');
        expected.push_str(line);
    }
    if let Some(e) = expr.take() {
        check(&e, &expected);
        checked += 1;
    }
    let total = golden.lines().filter(|l| l.starts_with("FullForm(")).count();
    assert_eq!(checked, total, "应有 {total} 条对拍");
}

fn check(full_expr: &str, expected: &str) {
    let inner = full_expr
        .strip_prefix("FullForm(")
        .and_then(|s| s.strip_suffix(')'))
        .expect("FullForm(...)");
    let mut env = Environment::new();
    // parse 以 `;` 结束(照 cyacas console 输入语义)
    let tree = parse_expression(&mut env, &format!("{inner};"))
        .unwrap_or_else(|_| panic!("parse {full_expr}"))
        .expect("非空");
    let actual = full_form(&tree);
    assert_eq!(
        actual, expected,
        "FullForm 不匹配: {full_expr}\n  期望: {expected:?}\n  实际: {actual:?}"
    );
}