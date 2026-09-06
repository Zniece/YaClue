//! golden 对拍(层 1+2):`tests/golden.txt` 由 cyacas `-pc` 逐条生成(见 `probes_l1.txt`,
//! 可再生:`cargo run --bin golden`)。
//!
//! 每条 expr 行按层 1+2 子集映射到 API(大整数加/乘/幂,浮点加/减/乘/除/精度 N,
//! 字面量直通),断言输出与 cyacas 字节一致。层 2 覆盖:除法(任意量级)、N(x,d) 精度、
//! <0.1 归一、e-形式。超越函数/有理数留在层 5+。

use yacas_rs::number::float::{Float, DEFAULT_PREC};
use yacas_rs::number::nat::Nat;

/// N(expr, prec) 壳:切出内层与精度。
fn eval_line(line: &str) -> String {
    let line = line.trim();
    if let Some(inner) = line.strip_prefix("N(").and_then(|s| s.strip_suffix(')')) {
        let comma = inner.rfind(',').expect("N: 缺精度参数");
        let expr = &inner[..comma];
        let prec: u32 = inner[comma + 1..].trim().parse().expect("N: 精度非数字");
        return eval_arith(expr, prec);
    }
    eval_arith(line, DEFAULT_PREC)
}

fn is_float(s: &str) -> bool {
    s.contains('.') || s.contains('e') || s.contains('E')
}

/// 层 1+2 迷你求值:`^`、`*`、`/`、`+`、`-`、字面量(可套括号)。
fn eval_arith(expr: &str, prec: u32) -> String {
    let e = expr.trim();
    let e = e.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(e);
    if let Some(i) = e.find('/') {
        let (a, b) = (e[..i].trim(), e[i + 1..].trim());
        let num = Float::from_decimal(a).expect("bad div num");
        let den = eval_arith(b, prec);
        let den = Float::from_decimal(&den).expect("bad div den");
        return num.div(&den, prec).expect("div by zero").format();
    }
    if let Some(i) = e.find('^') {
        // 幂:指数整数;幂后可能跟 +/- (如 2.5^2-6.25)
        let k = i + 1;
        let mut j = k;
        while j < e.len() && e.as_bytes()[j].is_ascii_digit() {
            j += 1;
        }
        let exp: u32 = e[k..j].parse().expect("bad exponent");
        let pow = if is_float(&e[..i]) {
            let base = Float::from_decimal(&e[..i]).expect("bad float pow base");
            let mut acc = base.clone();
            for _ in 1..exp {
                acc = acc.mul(&base, prec);
            }
            acc.format()
        } else {
            Nat::from_decimal(&e[..i])
                .unwrap_or_else(|| panic!("bad int pow base: {e}"))
                .pow(exp)
                .to_decimal()
        };
        let tail = &e[j..];
        if tail.is_empty() {
            return pow;
        }
        let op = &tail[..1];
        let operand = &tail[1..];
        let f1 = Float::from_decimal(&pow).expect("bad pow-tail lhs");
        let f2 = Float::from_decimal(operand).expect("bad pow-tail rhs");
        return if op == "-" {
            f1.sub(&f2, prec).format()
        } else {
            f1.add(&f2, prec).format()
        };
    }
    if let Some(i) = e.find('*') {
        let (a, b) = (e[..i].trim(), e[i + 1..].trim());
        return Float::from_decimal(a)
            .expect("bad mul lhs")
            .mul(&Float::from_decimal(b).expect("bad mul rhs"), prec)
            .format();
    }
    if let Some(i) = e.find('+') {
        // e+ 是字面量指数(如 1e+100),不是加法
        if i > 0 && matches!(e.as_bytes()[i - 1], b'e' | b'E') {
            return eval_literal(e);
        }
        let (a, b) = (e[..i].trim(), e[i + 1..].trim());
        if is_float(a) || is_float(b) {
            Float::from_decimal(a)
                .expect("bad add operand")
                .add(&Float::from_decimal(b).expect("bad add operand"), prec)
                .format()
        } else {
            Nat::from_decimal(a)
                .expect("bad int add")
                .add(&Nat::from_decimal(b).expect("bad int add"))
                .to_decimal()
        }
    } else if !e.starts_with('-') && e.contains('-') {
        let i = e.find('-').expect("dash");
        // e- 是字面量负指数(如 1e-100),不是减法
        if i > 0 && matches!(e.as_bytes()[i - 1], b'e' | b'E') {
            return eval_literal(e);
        }
        let (a, b) = (e[..i].trim(), e[i + 1..].trim());
        Float::from_decimal(a)
            .expect("bad sub operand")
            .sub(&Float::from_decimal(b).expect("bad sub operand"), prec)
            .format()
    } else {
        eval_literal(e)
    }
}

/// 字面量分支:浮点文本直通 / 整数。
fn eval_literal(e: &str) -> String {
    if is_float(e) {
        Float::from_decimal(e).expect("bad literal").format()
    } else {
        Nat::from_decimal(e).expect("bad int literal").to_decimal()
    }
}

#[test]
fn golden_layer1() {
    let golden = include_str!("golden.txt");
    let mut checked = 0;
    for line in golden.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (expr, expected) = line.split_once('\t').expect("golden row: expr<TAB>out");
        let actual = eval_line(expr);
        assert_eq!(
            actual, expected,
            "golden 不匹配: {expr} 应输出 {expected},实得 {actual}"
        );
        checked += 1;
    }
    let total = golden.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(checked, total, "应有 {total} 行对拍");
}