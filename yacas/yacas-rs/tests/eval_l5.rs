//! 层 5 验收:eval 闭环(变量/规则/模式/兜底/列表)与 cyacas golden_l5.txt 逐字节对拍。
//!
//! probes_l5.txt 与 golden_l5.txt 同一会话顺序:`>` 前缀行 = setup(定义规则/赋值,
//! 执行但不记录),其余行 = 验收行 —— 两侧按文件顺序用同一个 Environment 执行,
//! 保证状态一致(规则定义影响后续求值)。
//!
//! 6b-7 改造:setup 行的 `<--`/`#` 本是 patterns.rep/code.ys 的脚本规则 —— 现已直通
//! (加载 code.ys 后直接 eval),删除原 apply_setup 的手写 API 翻译(照契约,脚本直通为准)。

use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

#[test]
fn golden_eval_l5() {
    let golden = include_str!("golden_l5.txt");
    let probes = include_str!("probes_l5.txt");
    let mut env = Environment::new();

    // 6b-7:setup 的 `<--`/`#` 为 code.ys 脚本规则,加载后即可直通 eval;
    // 另注 Nth(standard.ys 规则 10)在层 7 由脚本接手,当前以 Rust 注册顶上。
    let code_ys = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/patterns.rep/code.ys");
    yacas_rs::standard::internal_load(&mut env, code_ys).unwrap();
    // T2:`:=` 脚本化 —— MakeVector 脚本体用 :=(res:={};i:=1;),`#`-定义链经 MakeVector
    // 依赖 deffunc;无 deffunc 时 := 无规则 → res 未绑 → InvalidArg。统一装载序装载。
    let deffunc_ys = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/deffunc.rep/code.ys");
    yacas_rs::standard::internal_load(&mut env, deffunc_ys).unwrap();
    // T3:Nth 由 standard.ys 接管(原 Rust 手写 bootstrap_nth 已退役)。
    let standard_ys = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/standard.ys");
    yacas_rs::standard::internal_load(&mut env, standard_ys).unwrap();

    // golden 记录读取:层 5 探针行为纯表达式、结果行恒单行(InfixPrinter 不换行)
    // -> 每条记录 = 一行 `expr<TAB>结果`。
    let mut golden_exprs: Vec<String> = Vec::new();
    let mut golden_outs: Vec<String> = Vec::new();
    for line in golden.lines() {
        if let Some(tab) = line.find('\t') {
            golden_exprs.push(line[..tab].to_string());
            golden_outs.push(line[tab + 1..].to_string());
        }
    }

    let mut checked = 0usize;
    for line in probes.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(setup) = line.strip_prefix('>') {
            // setup 直通 eval(原 apply_setup 手写翻译已删;`<--`/`#`/`:=` 均脚本/核心命令直通)。
            run(&mut env, setup);
            continue;
        }
        let actual = run(&mut env, line);
        let expected = &golden_outs[checked];
        assert_eq!(
            actual, *expected,
            "eval 不匹配({}): 期望 {:?}, 实际 {:?}",
            golden_exprs.get(checked).map(|s| s.as_str()).unwrap_or("?"),
            expected,
            actual
        );
        checked += 1;
    }
    assert_eq!(checked, golden_exprs.len(), "应核对 {} 条", golden_exprs.len());
}

/// parse(尾部分号)+ eval + InfixPrinter 打印(cyacas 结果行格式)。
fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| {
        eprintln!("env locals: {:?}", env.locals.as_ref().map(|f| f.first.as_ref().map(|n| n.variable.as_ref().to_string())));
        eprintln!("env globals: {:?}", env.globals.keys().map(|k| k.as_ref().to_string()).collect::<Vec<_>>());
        panic!("eval {src}: {e:?} (tree={})", infix_print(env, &tree))
    });
    infix_print(env, &result)
}