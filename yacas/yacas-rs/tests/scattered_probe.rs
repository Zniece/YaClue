//! 零散核心命令补位探针(与 cyacas oracle 锁步,2026-09 补位专项):
//! 布尔/数值(BitXor/StrictTotalOrder/MathIsSmall/FromBase/ToBase/CharString)、
//! 系统信息(Version/Interpreter/Variables/FindFile/FindFunction/MaxEvalDepth/
//! GarbageCollect/InDebugMode/DebugFile)、Pretty×4、CurrentFile/CurrentLine、
//! XmlExplodeTag/PatchString/LispRead(读族经 FromString 隔离)。
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
fn scattered_probe() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    run(&mut env, "Load(\"yacasinit.ys\")");
    // (探针, cyacas 期望;ERR(...) = cyacas 报 Invalid argument)
    let probes: [(&str, &str); 44] = [
        ("MathNot(True)", "False"),
        ("MathNot(False)", "True"),
        ("BitXor(10,13)", "7"),
        ("BitXor(5,3)", "6"),
        ("StrictTotalOrder(1,2)", "True"),
        ("StrictTotalOrder(2,1)", "False"),
        ("StrictTotalOrder(\"a\",\"b\")", "True"),
        ("StrictTotalOrder(1,1.0)", "False"),
        ("StrictTotalOrder(x,y)", "True"),
        ("MathIsSmall(5)", "True"),
        ("MathIsSmall(-5)", "True"),
        ("MathIsSmall(2^52)", "True"),
        ("MathIsSmall(2^53)", "False"),
        ("MathIsSmall(2^100)", "False"),
        ("MathIsSmall(1.5)", "True"),
        ("MathIsSmall(1e1020)", "True"),
        ("MathIsSmall(1e1021)", "False"),
        ("MathIsSmall(N(2.5,16))", "True"),
        ("CharString(65)", "\"A\""),
        ("CharString(97)", "\"a\""),
        ("CharString(300)", "\",\""),
        ("FromBase(2,\"1010\")", "10"),
        ("FromBase(16,\"1F\")", "31"),
        ("FromBase(2,\"1.1\")", "1.5"),
        ("FromBase(10,\"123\")", "123"),
        ("FromBase(2,\"0.101\")", "0.625"),
        ("FromBase(2,\"10.101\")", "2.625"),
        ("FromBase(16,\"1.f\")", "1.9375"),
        ("FromBase(3,\"0.1\")", "0.3333333333333333"),
        ("FromBase(10,\"0.2222222222222222222\")", "0.2222222222222222222"),
        ("FromBase(2,\"1.1e2\")", "0.15e3"),
        ("FromBase(16,\"1e5\")", "485"),
        ("FromBase(16,1)", "ERR(InvalidArg)"),
        ("FromBase(33,\"1\")", "ERR(InvalidArg)"),
        ("ToBase(2,10)", "\"1010\""),
        ("ToBase(16,255)", "\"ff\""),
        ("ToBase(32,255)", "\"7v\""),
        ("ToBase(31,255)", "\"87\""),
        ("ToBase(2,3.5)", "\"11.1\""),
        ("ToBase(10,0.)", "\"0\""),
        ("ToBase(2,0.5)", "\"0.1\""),
        ("ToBase(16,1.5)", "\"1.8\""),
        ("ToBase(10,3.0)", "\"3.\""),
        ("ToBase(2,-0.5)", "\"-0.1\""),
    ];
    // 已知偏差(浮点算术保真位数:我 10 位 vs cyacas ~30 位;显示层一致,
    // ToBase 读原始尾数分叉,记 historical porting notes):ToBase(2,1.0/3.0)、ToBase(3,1.0/3.0)
    for (src, want) in probes {
        let got = run(&mut env, src);
        assert_eq!(got, want, "探针 {src}");
    }
    // —— 长分数串探针(与上表分开,便于定位)——
    assert_eq!(run(&mut env, "ToBase(2,0.1)"), "\"0.000110011001100110011001100110011\"");
    assert_eq!(run(&mut env, "ToBase(3,0.5)"), "\"0.1111111111111111111111111111111111\"");
    assert_eq!(run(&mut env, "ToBase(5,0.3)"), "\"0.1222222222222222222222222222222222\"");
    // —— 系统信息/状态族 ——
    assert_eq!(run(&mut env, "Version()"), "\"\"");
    assert_eq!(run(&mut env, "Interpreter()"), "\"yacas\"");
    assert_eq!(run(&mut env, "GarbageCollect()"), "True");
    assert_eq!(run(&mut env, "InDebugMode()"), "False");
    assert_eq!(run(&mut env, "MaxEvalDepth(100)"), "True");
    assert_eq!(run(&mut env, "DebugFile(1)"), "ERR(Generic(\"Cannot call DebugFile in non-debug version of Yacas\"))");
    assert_eq!(run(&mut env, "DebugLine(1)"), "ERR(Generic(\"Cannot call DebugLine in non-debug version of Yacas\"))");
    // FindFile 裸名优先命中 = CWD 相对(oracle 探针 cwd=scripts);测试切 cwd 对齐
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(d).unwrap();
    assert_eq!(run(&mut env, "FindFile(\"yacasinit.ys\")"), "\"yacasinit.ys\"");
    std::env::set_current_dir(old_cwd).unwrap();
    assert_eq!(run(&mut env, "FindFile(\"nonexistent.ys\")"), "\"\"");
    assert_eq!(run(&mut env, "FindFunction(\"NoSuchFn\")"), "Empty");
    // DefLoad 登记在 boot 即存在:触发**前**可查到定义文件(oracle 实证);
    // 懒触发摘挂 iFileToOpen(cyacas GetUserFunction 同款)→ 触发后查得 Empty
    assert_eq!(run(&mut env, "FindFunction(\"MathSqrt\")"), "\"stdfuncs.rep/elemfuncs.ys\"");
    assert_eq!(run(&mut env, "MathSqrt(2)"), "1.4142135623");
    assert_eq!(run(&mut env, "FindFunction(\"MathSqrt\")"), "Empty");
    // —— Pretty 族(Set 存带引号串;Get 未设 → "")——
    assert_eq!(run(&mut env, "PrettyReader'Get()"), "\"\"");
    assert_eq!(run(&mut env, "PrettyReader'Set(\"x\")"), "True");
    assert_eq!(run(&mut env, "PrettyReader'Get()"), "\"x\"");
    assert_eq!(run(&mut env, "PrettyReader'Set()"), "True");
    assert_eq!(run(&mut env, "PrettyReader'Get()"), "\"\"");
    assert_eq!(run(&mut env, "PrettyPrinter'Set(\"TeXForm\")"), "True");
    assert_eq!(run(&mut env, "PrettyPrinter'Get()"), "\"TeXForm\"");
    assert_eq!(run(&mut env, "PrettyPrinter'Set()"), "True");
    assert_eq!(run(&mut env, "PrettyPrinter'Get()"), "\"\"");
    // —— CurrentFile/CurrentLine ——
    assert_eq!(run(&mut env, "CurrentFile()"), "\"CommandLine\"");
    assert_eq!(run(&mut env, "CurrentLine()"), "1");
    assert_eq!(run(&mut env, "FromString(\"1;\")CurrentFile();"), "\"String\"");
    // —— XmlExplodeTag ——
    assert_eq!(
        run(&mut env, "XmlExplodeTag(\"<a href=\\\"x\\\">\")"),
        "XmlTag(\"A\",{{\"HREF\",\"x\"}},\"Open\")"
    );
    assert_eq!(run(&mut env, "XmlExplodeTag(\"</a>\")"), "XmlTag(\"A\",{},\"Close\")");
    assert_eq!(run(&mut env, "XmlExplodeTag(\"<br/>\")"), "XmlTag(\"BR\",{},\"OpenClose\")");
    assert_eq!(run(&mut env, "XmlExplodeTag(\"abc\")"), "\"abc\"");
    // —— PatchString('段内 Echo 输出走当前输出')——
    assert_eq!(run(&mut env, "PatchString(\"a<?Echo(1+2);?>b\")"), "\"a3 \nb\"");
    // —— LispRead 族(纯前缀解析;无 ';' 校验)——
    assert_eq!(run(&mut env, "FromString(\"1+2;\")LispRead()"), "1");
    assert_eq!(run(&mut env, "FromString(\"(f 1 2);\")LispRead()"), "f(1,2)");
    assert_eq!(run(&mut env, "FromString(\"{1,2};\")LispRead()"), "{");
    assert_eq!(run(&mut env, "FromString(\"(1,2);\")LispReadListed()"), "{1,,,2}");
    // —— xml 词法器切换 + ReadToken ——
    assert_eq!(run(&mut env, "FromString(\"<a> x <b>\")[XmlTokenizer();ReadToken();];"), "<a>");
    assert_eq!(run(&mut env, "FromString(\"<a> x <b>\")[XmlTokenizer();ReadToken();ReadToken();];"), " x ");
    assert_eq!(run(&mut env, "FromString(\"1+2;\")[DefaultTokenizer();Read();];"), "1+2");
    // —— MathAnd/MathOr/MathNot 别名(与 And/Or/Not 同体)——
    assert_eq!(run(&mut env, "MathAnd(True,False)"), "False");
    assert_eq!(run(&mut env, "MathOr(True,False)"), "True");
    assert_eq!(run(&mut env, "MathAnd(1,2)"), "MathAnd(1,2)");
    // MaxEvalDepth(1):设上限成功;其后任何嵌套求值即达上限(max=1 毒化,同 cyacas
    // oracle:MaxEvalDepth(1) 后 1+1 也死)→ 放本测试最末
    assert_eq!(run(&mut env, "MaxEvalDepth(1)"), "True");
}

/// TraceRule 的 TrEnter/TrLeave 输出(走输出缓冲,单独验证)。
#[test]
fn trace_rule_output() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    run(&mut env, "Load(\"yacasinit.ys\")");
    run(&mut env, "g(_x) <-- x*2;");
    let depth = env.push_output();
    let src = "TraceRule(g(_x), g(3))";
    let t = parse_expression(&mut env, &format!("{src};")).unwrap().expect("非空");
    let r = eval(&mut env, &t).unwrap();
    let captured = env.pop_output(depth);
    assert_eq!(infix_print(&env, &r), "6");
    // 缩进 = eval_depth×2(本引擎 boot 后该点深度 2 → 4 空格;cyacas 控制台 ~14 层
    // → 28 空格,绝对值随控制台引导深度,语义一致故按内容+相对缩进断言)
    let text = &captured.text;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "TrEnter/TrArg/TrLeave 三行,实得 {text}");
    assert!(lines[0].trim_start().starts_with("TrEnter(\"g\",\"g(3)\",\"\",0);"), "{text}");
    assert!(lines[1].trim_start().starts_with("TrArg(\"3\",\"3\");"), "{text}");
    assert!(lines[2].trim_start().starts_with("TrLeave(\"g(3)\",\"6\");"), "{text}");
    let ind0 = lines[0].len() - lines[0].trim_start().len();
    let ind1 = lines[1].len() - lines[1].trim_start().len();
    let ind2 = lines[2].len() - lines[2].trim_start().len();
    assert_eq!(ind1, ind0 + 4, "TrArg 缩进 = TrEnter+2 层");
    assert_eq!(ind2, ind0, "TrLeave 缩进 = TrEnter");
}

/// MathDebugInfo 的 dump 输出(走输出缓冲,单独验证)。
#[test]
fn math_debug_info_output() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    run(&mut env, "Load(\"yacasinit.ys\")");
    let depth = env.push_output();
    let src = "MathDebugInfo(5.)";
    let t = parse_expression(&mut env, &format!("{src};")).unwrap().expect("非空");
    eval(&mut env, &t).unwrap();
    let captured = env.pop_output(depth);
    assert_eq!(
        captured.text,
        "Number:\n1 words, 0 after point (x10^0), 10-prec 10\n 0000 0000 0000 0000 0000 0000 0000 0101\n"
    );
    let depth = env.push_output();
    let src = "MathDebugInfo(5)";
    let t = parse_expression(&mut env, &format!("{src};")).unwrap().expect("非空");
    eval(&mut env, &t).unwrap();
    let captured = env.pop_output(depth);
    assert_eq!(captured.text, "No number representation\n");
}
