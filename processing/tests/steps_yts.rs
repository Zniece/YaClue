//! 步骤层规范 runner。
//!
//! 步骤层(steps.rep + 其 .yts 规范)归 processing 所有后,规范随层走:
//! 本测试把 `tests/steps/` 下的规范模块按清单顺序逐语句求值,断言类语句
//! (`Verify(expr, expected)` / `TestYacas(expr, expected)`)结果必须为 True。
//! 两条断言命令的语义来自上游标准库 testers.rep(由 runner 显式装载),
//! .yts 内容保持原样,不为 runner 改写。所有模块共用同一引擎环境,
//! common.yts 定义的 helper 跨模块可见;新模块在 SPEC_FILES 追加条目。

use processing::engine::{Engine, RustEngine};

/// 去掉 /* ... */ 块注释与 // 行注释(字符串字面量内的内容不动)
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                for d in chars.by_ref() {
                    out.push(d);
                    if d == '"' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(d) = chars.next() {
                    if d == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for d in chars.by_ref() {
                    if d == '\n' {
                        out.push(d);
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// 按"括号深度 0 处的分号"切分顶层语句(字符串与括号内容不分)
fn split_statements(src: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    for c in src.chars() {
        match c {
            '"' => {
                in_string = !in_string;
                cur.push(c);
            }
            '(' | '[' | '{' if !in_string => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' if !in_string => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ';' if !in_string && depth == 0 => {
                if !cur.trim().is_empty() {
                    stmts.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        stmts.push(cur.trim().to_string());
    }
    stmts
}

const IS_ASSERTION_PREFIXES: [&str; 2] = ["Verify(", "TestYacas("];

/// 规范模块(顺序即装载序;common 定义跨模块共享的 helper)
const SPEC_FILES: [(&str, &str); 9] = [
    ("common.yts", include_str!("steps/common.yts")),
    ("deriv.yts", include_str!("steps/deriv.yts")),
    (
        "integral_basic.yts",
        include_str!("steps/integral_basic.yts"),
    ),
    ("integral_usub.yts", include_str!("steps/integral_usub.yts")),
    (
        "integral_parts.yts",
        include_str!("steps/integral_parts.yts"),
    ),
    (
        "integral_partial_fractions.yts",
        include_str!("steps/integral_partial_fractions.yts"),
    ),
    (
        "integral_trig_sub.yts",
        include_str!("steps/integral_trig_sub.yts"),
    ),
    (
        "integral_definite.yts",
        include_str!("steps/integral_definite.yts"),
    ),
    ("texts.yts", include_str!("steps/texts.yts")),
];

// 全量逐语句规范门禁，本机约 40 秒；普通 workspace 测试必须覆盖。
#[test]
fn steps_yts_spec_all_green() {
    let mut engine = RustEngine::spawn().expect("RustEngine 启动失败");
    // testers.rep 属上游标准库,提供 Verify/TestYacas(启动链已登记,须用 Use 装载)
    engine
        .eval("Use(\"testers.rep/code.ys\")")
        .expect("装载 testers.rep 失败");

    let mut asserted = 0;
    for (name, src) in SPEC_FILES {
        let statements = split_statements(&strip_comments(src));
        assert!(statements.len() >= 2, "{name} 应切出 2+ 条语句");
        for (i, stmt) in statements.iter().enumerate() {
            let is_assertion = IS_ASSERTION_PREFIXES.iter().any(|p| stmt.starts_with(p));
            let result = engine.eval(stmt).unwrap_or_else(|e| {
                panic!("{name} 第 {i} 句求值失败: {stmt}\n{e}");
            });
            if is_assertion {
                asserted += 1;
                assert_eq!(
                    result.expr.to_string(),
                    "True",
                    "{name} 第 {i} 句断言未通过: {stmt}"
                );
            }
        }
    }
    assert!(asserted >= 30, "断言语句应有 30+ 条,实际 {asserted}");
}
