//! CustomEval 族对拍测试(照 cyacas LispCustomEval + TracedEvaluator + DefaultDebugger)。
//!
//! CustomEval(entercb,leavecb,errorcb,expr):设 debugger,求值 expr 期间每子表达式
//! (含原子/核心命令/用户函数)先 Enter(entercb,可读 CustomEval'Expression)后 Leave
//! (leavecb,可读 CustomEval'Result);结束清 debugger 返回 expr 结果。
//! 结构对照 cyacas TraceExp oracle:1+1 → x/1 原子 → IsNumber(x) → MathAdd → 2。
//! 配套:Customeval'Expression/Result/Stop 无调试器时报错(TrapError 捕获);
//! CustomEval'Locals 无调试器返回 {}。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("ok");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}

#[test]
fn custom_eval_family_oracle() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let probes = [
        // 配套命令无调试器:报错被 TrapError 捕获(cyacas oracle:err-gexpr 等)
        (r#"TrapError(CustomEval'Expression(), "err")"#, "\"err\""),
        (r#"TrapError(CustomEval'Result(), "err")"#, "\"err\""),
        (r#"TrapError(CustomEval'Stop(), "err")"#, "\"err\""),
        // Locals 无调试器也返回 {}(cyacas oracle)
        ("CustomEval'Locals()", "{}"),
        // CustomEval 基本:回调不影响结果
        ("CustomEval(True,True,True,1+1)", "2"),
        ("[x := 9; CustomEval(True,True,True,x+1);]", "10"),
        // CustomEval 清除:调用后 debugger 复位,Expression 又报错
        (r#"[CustomEval(True,True,True,1); TrapError(CustomEval'Expression(), "gone");]"#, "\"gone\""),
    ];
    for (p, exp) in probes {
        let got = run(&mut e, p);
        assert_eq!(got, exp, "probe {p}");
    }
}

#[test]
fn trace_hook_structure() {
    // 钩子结构:用 WriteString 直打 top_expr/top_result(避开脚本 Echo),
    // 验证 Enter/Leave 对每个子表达式触发、Expression/Result 取值正确。
    // cyacas TraceExp(1+1) oracle 结构:1+1 → 1 → IsNumber(x) → MathAdd(x,y) → 2。
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    let src = r#"ToString()[CustomEval(
        [WriteString("E:"); Write(CustomEval'Expression()); WriteString(";");],
        [WriteString("L:"); Write(CustomEval'Result()); WriteString(";");],
        True, 1+1);]"#;
    let got = run(&mut e, src);
    // 开头 Enter 1+1;进入其子表达式;结束 Leave 2(1+1 结果)
    assert!(got.starts_with("\"E:1+1;"), "应先 Enter 顶层 1+1, got={got}");
    assert!(got.ends_with("L:2;\""), "应末 Leave 2, got={got}");
    assert!(got.contains("E:MathAdd(x,y);"), "应跟踪 MathAdd 调用");
    assert!(got.contains("L:True;"), "IsNumber 等谓词结果 True");
}
