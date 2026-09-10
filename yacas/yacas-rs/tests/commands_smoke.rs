//! 命令层冒烟测试(层 6 开端):核心命令直接 eval 验证。
//! 用 parse+eval+infix_print 链,断言与 yacas 语义一致(无 golden;逐断言)。

use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree)
        .unwrap_or_else(|e| panic!("eval {src}: {e:?} (tree={})", infix_print(env, &tree)));
    infix_print(env, &result)
}

/// 统一装载序(照 yacasinit.ys:36-45:patterns → deffunc → standard → stdarith)。
fn boot(env: &mut Environment) {
    for f in [
        "patterns.rep/code.ys",
        "deffunc.rep/code.ys",
        "standard.ys",
        "stdarith.ys",
        "stubs.rep/code.ys",
    ] {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
        yacas_rs::standard::internal_load(env, &format!("{p}{f}")).unwrap();
    }
}

#[test]
fn set_and_variable() {
    let mut env = Environment::new();
    // T2:`:=` 由 scripts(deffunc.rep/code.ys)接管 —— 用前先装载(照 cyacas 装载序)。
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, deffunc_ys).unwrap();
    assert_eq!(run(&mut env, "aa := 5"), "5");
    assert_eq!(run(&mut env, "aa"), "5");
    assert_eq!(run(&mut env, "aa := bb"), "bb");
    assert_eq!(run(&mut env, "aa"), "bb");
}

#[test]
fn local_shadowing() {
    let mut env = Environment::new();
    // T2:`:=` 脚本化(deffunc.rep/code.ys)—— 用前先装载。
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, deffunc_ys).unwrap();
    assert_eq!(run(&mut env, "aa := 1"), "1");
    assert_eq!(run(&mut env, "[Local(aa); aa := 2; aa;]"), "2");
    assert_eq!(run(&mut env, "aa"), "1");
}

#[test]
fn local_symbols_use_the_language_standard_spelling_and_shared_generation() {
    let mut env = Environment::new();
    assert_eq!(run(&mut env, "LocalSymbols(a,b)({a,b})"), "{$a1,$b1}");
    assert_eq!(run(&mut env, "LocalSymbols(a)(a)"), "$a2");
}

#[test]
fn if_not_equals() {
    let mut env = Environment::new();
    boot(&mut env); // Equals(1+2,3) 需 stdarith 的 + 规则(照 cyacas 控制台序)
    assert_eq!(run(&mut env, "If(True, 1, 2)"), "1");
    assert_eq!(run(&mut env, "If(False, 1, 2)"), "2");
    assert_eq!(run(&mut env, "If(False, 1)"), "False");
    assert_eq!(run(&mut env, "Not(True)"), "False");
    assert_eq!(run(&mut env, "Equals(aa, aa)"), "True");
    assert_eq!(run(&mut env, "Equals(aa, bb)"), "False");
    assert_eq!(run(&mut env, "Equals(1+2, 3)"), "True");
}

#[test]
fn analytic_function_parity_normalizes_negative_coefficients() {
    let mut env = Environment::new();
    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    assert_eq!(
        run(&mut env, &format!("DefaultDirectory(\"{scripts}\")")),
        "True"
    );
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    for (source, expected) in [
        ("Sin(-5*x)", "-Sin(5*x)"),
        ("Cos(-5*x)", "Cos(5*x)"),
        ("Tan(-5*x)", "-Tan(5*x)"),
        ("ArcSin(-5*x)", "-ArcSin(5*x)"),
        ("ArcTan(-5*x)", "-ArcTan(5*x)"),
    ] {
        assert_eq!(run(&mut env, source), expected, "{source}");
    }
}

#[test]
fn head_tail_length_listify_string_type() {
    let mut env = Environment::new();
    assert_eq!(run(&mut env, "Head({aa,bb,cc})"), "aa"); // 照 cyacas 实测(6a 曾错返 "List")
    assert_eq!(run(&mut env, "Tail({aa,bb,cc})"), "{bb,cc}");
    assert_eq!(run(&mut env, "Length({aa,bb,cc})"), "3");
    // cyacas 实测 Listify({a,b,c}) → {List,a,b,c}(保留原内链头;6a 时期断言
    // "{aa,bb}" 是手写错用,当时 cmd_listify 丢 List 头;已修,断言随 cyacas)。
    assert_eq!(run(&mut env, "Listify({aa,bb})"), "{List,aa,bb}");
    assert_eq!(run(&mut env, "Type({aa,bb})"), "\"List\""); // cyacas 实测:调用树→带引号 head 串
    assert_eq!(run(&mut env, "Type(aa)"), "\"\""); // cyacas 实测:原子/数字→空串
                                                   // cyacas 实测:String("xx") → ""xx""(无条件包引号,实参文本已含引号再包一层;
                                                   // 6a 期断言 "\"xx\"" 是手写错用)。String(aa) → "aa"(单层)。
    assert_eq!(run(&mut env, "String(\"xx\")"), "\"\"xx\"\"");
}

#[test]
fn while_loop() {
    let mut env = Environment::new();
    // `i<3` / `i+1` 需 stdarith 算术规则,`:=` 需 deffunc —— 统一 boot。
    boot(&mut env);
    assert_eq!(
        run(&mut env, "[Local(i); i := 0; While(i<3) i := i+1; i;]"),
        "3"
    );
}

#[test]
fn nth_from_standard_ys() {
    // T3:Nth 规则 10 由 standard.ys 脚本装载(原 Rust 手写 bootstrap_nth 已退役)——
    // 同探针复测,验证脚本版 Nth 与旧手写版行为一致(照 cyacas 实测)。
    let mut env = Environment::new();
    let code_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/patterns.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, code_ys).unwrap();
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, deffunc_ys).unwrap();
    let standard_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/standard.ys"
    );
    yacas_rs::standard::internal_load(&mut env, standard_ys).unwrap();
    // 探针(cyacas 实测):Nth({a,b,c},1) → a;Nth({a,b,c},2) → b —— 1 起。
    assert_eq!(run(&mut env, "Nth({aa,bb,cc},1)"), "aa");
    assert_eq!(run(&mut env, "Nth({aa,bb,cc},2)"), "bb");
    // 谓词不命中(实参非函数调用)——保持,但 InfixPrinter 的 nth 分支(照 Java
    // InfixPrinter 172-179)把 `Nth(a,b)` 打印成 `a[b]`,故输出为 `xx[1]`:
    assert_eq!(run(&mut env, "Nth(xx,1)"), "xx[1]");
    // 防自匹配第三支的触发场景是"实参求值后**仍是** Nth 形状"——本用列中 Nth 为
    // 非宏 BranchingUserFunction(实参先求值):内层 Nth({aa},1)→aa,外层第一支
    // IsFunction(aa)=False 即挡住 → 保持 → 实参求值后重包 Nth(aa,0)→ 打印 aa[0]
    // (cyacas 实测 aa[0] ✓):
    assert_eq!(run(&mut env, "Nth(Nth({aa},1),0)"), "aa[0]");
    // Listify 保留原内链头(cyacas 实测 Listify({a,b,c}) → {List,a,b,c};Head(Listify(f(x))) → f):
    assert_eq!(run(&mut env, "Listify({aa,bb})"), "{List,aa,bb}");
    assert_eq!(run(&mut env, "Head(Listify(f(x)))"), "f");
}

#[test]
fn rule_chain_code_ys_load() {
    // 6b-6:加载 patterns.rep/code.ys(<--/#/DefinePattern 脚本规则链),`10 # f(_x) <-- body`
    // 直接可 eval。MakeVector 改为脚本定义(cyacas 实测 {arg1,arg2,arg3} —— 1 起,撤内置)。
    let mut env = Environment::new();
    // g1(5)→2*5→10 需 stdarith 乘法规则;`:='` 需 deffunc —— 统一 boot。
    boot(&mut env);
    // 注:本批已按 cyacas 探针修四处真漂移(find_local 跨帧 / Atom 去引号 /
    // String eval+无条件包引号 / Insert 空表);此处按探针锁定的命令行为核对:
    // String(aa)→"aa"、Atom(ConcatStrings("aa","3"))→aa3、Insert({},1,zz)→{zz}。
    assert_eq!(run(&mut env, "String(aa)"), "\"aa\"");
    assert_eq!(run(&mut env, "ConcatStrings(\"aa\",\"3\")"), "\"aa3\"");
    assert_eq!(run(&mut env, "DestructiveInsert({},1,zz)"), "{zz}");
    assert_eq!(run(&mut env, "DestructiveReverse({zz,11})"), "{11,zz}");
    // MakeVector 体逐条对照(cyacas 实测 {arg1,arg2,arg3}):形参用局部逐字模拟体,
    // 前缀 vec(ii3 递增 1..3)即 {vec1,vec2,vec3} —— 锁破坏族插入/逆序写回循环;
    // 旧期望 "{arg1,arg2,arg3}" 系从下行 MakeVector 误抄,已照契约§7-C12 修正。
    assert_eq!(run(&mut env, "[Local(rr3,ii3,dd3); rr3 := {}; ii3 := 1; dd3 := 3; Set(dd3,MathAdd(dd3,1)); While(LessThan(ii3,dd3)) [ DestructiveInsert(rr3,1,Atom(ConcatStrings(String(vec),String(ii3)))); Set(ii3,MathAdd(ii3,1)); ]; DestructiveReverse(rr3); rr3;]"), "{vec1,vec2,vec3}");
    // 探针(cyacas 实测):MakeVector(arg,3) → {arg1,arg2,arg3}(脚本版,1 起;
    // 含 _ 名不可作 Set 第一实参 —— cyacas 分词为子列表,InvalidArg;名须单原子)。
    assert_eq!(run(&mut env, "MakeVector(arg,3)"), "{arg1,arg2,arg3}");
    // 探针(cyacas 实测):`10 # g1(_x) <-- 2*x;` → True;g1(5) → 10
    assert_eq!(run(&mut env, "10 # g1(_x) <-- 2*x;"), "True");
    assert_eq!(run(&mut env, "g1(5)"), "10");
    // 双实参(照 DefinePattern 自动建底):`10 # g2(_x,_y) <-- x+y;` g2(40,2) → 42
    assert_eq!(run(&mut env, "10 # g2(_x,_y) <-- x+y;"), "True");
    assert_eq!(run(&mut env, "g2(40,2)"), "42");
    // 优先级:cyacas 实测 `10#`(先定)+`20#`(后定)→ g3(5) 返 1 —— 数字小者先试;
    // 又实测同优先(两条 10#,后定 100)→ 后定义者胜(返 100)。断言照实测抄(契约 C12)。
    assert_eq!(run(&mut env, "20 # g3(_x) <-- 100;"), "True");
    assert_eq!(run(&mut env, "10 # g3(_x) <-- 1;"), "True");
    assert_eq!(run(&mut env, "g3(5)"), "1");
}

#[test]
fn macro_hold_arg_math_command_reeval() {
    // cyacas 实测锁(契约 §7.B.6):宏形参保持树在数学命令保持路径**再 eval 一次** —
    // `10#f(_x)<--2*x; 10#G(_x,_y)<--x+y; G(f(aa),bb);` → 2*aa+bb
    // (保持形态 = 原 head `+` + 二次 eval 后实参;非 List 头装载、非原实参重包)。
    let mut env = Environment::new();
    let code_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/patterns.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, code_ys).unwrap();
    // T2:`:=` 脚本化 —— 统一装载 deffunc(与 rule_chain_code_ys_load 一致;# 探针同环境)。
    // T3:Nth 由 standard.ys 接管(bootstrap_nth 退役)。
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(&mut env, deffunc_ys).unwrap();
    let standard_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/standard.ys"
    );
    yacas_rs::standard::internal_load(&mut env, standard_ys).unwrap();
    assert_eq!(run(&mut env, "10 # f(_x) <-- 2*x;"), "True");
    assert_eq!(run(&mut env, "10 # G(_x,_y) <-- x+y;"), "True");
    assert_eq!(run(&mut env, "G(f(aa),bb)"), "2*aa+bb");
}

#[test]
fn math_div_arbitrary_precision() {
    // 向零截断(契约:MathDiv(-7,3) = -2,Rem 依赖 n - m*Div(n,m) 的符号约定)。
    // 需装载脚本:裸环境下 `-` 不折算,负字面量到不了命令层。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "MathDiv(7, 3)"), "2");
    assert_eq!(run(&mut env, "MathDiv(-7, 3)"), "-2");
    assert_eq!(run(&mut env, "MathDiv(7, -3)"), "-2");
    assert_eq!(run(&mut env, "MathDiv(-7, -3)"), "2");
    // 超出 i64:曾因 i64 解析失败抛 InvalidArg(2^64 = 18446744073709551616)
    // i64::MIN / -1 溢出:不得回绕成 i64::MIN,应为 2^63
    assert_eq!(
        run(&mut env, "MathDiv(-9223372036854775808, -1)"),
        "9223372036854775808"
    );
    assert_eq!(
        run(&mut env, "MathDiv(18446744073709551616, 2)"),
        "9223372036854775808"
    );
    assert_eq!(
        run(&mut env, "MathDiv(-18446744073709551616, 3)"),
        "-6148914691236517205"
    );
    assert_eq!(
        run(
            &mut env,
            "MathDiv(340282366920938463463374607431768211456, 4294967296)"
        ),
        "79228162514264337593543950336"
    );
    // 商为 0 时不得输出 "-0"
    assert_eq!(
        run(
            &mut env,
            "MathDiv(18446744073709551615, 18446744073709551617)"
        ),
        "0"
    );
    assert_eq!(
        run(
            &mut env,
            "MathDiv(-18446744073709551615, 18446744073709551617)"
        ),
        "0"
    );
    // 零除数 / 非整数操作数:报错不崩溃
    let tree = parse_expression(&mut env, "MathDiv(1, 0);")
        .unwrap()
        .unwrap();
    assert!(eval(&mut env, &tree).is_err());
    let tree = parse_expression(&mut env, "MathDiv(2.5, 2);")
        .unwrap()
        .unwrap();
    assert!(eval(&mut env, &tree).is_err());
}

#[test]
fn equal_precedence_later_rule_wins() {
    // 契约(同优先级):后定义者先试。回归锚点:insert_rule 曾在同优先级块内
    // 按 mid 落点插入,规则顺序随块大小漂移 —— 三条 10# 规则的定义顺序
    // 与命中顺序在大表场景下会翻转(steps.rep 的规则表正是这种形态)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "10 # h1(_x) <-- 1;"), "True");
    assert_eq!(run(&mut env, "10 # h1(_x) <-- 2;"), "True");
    assert_eq!(run(&mut env, "10 # h1(_x) <-- 3;"), "True");
    assert_eq!(run(&mut env, "h1(0)"), "3");
    // 与块大小无关:先堆 30 条同优先级规则,再定义的决定性规则仍应胜出
    for i in 0..30 {
        assert_eq!(
            run(&mut env, format!("10 # h2(_x) <-- {i};").as_str()),
            "True"
        );
    }
    assert_eq!(run(&mut env, "10 # h2(_x) <-- 999;"), "True");
    assert_eq!(run(&mut env, "h2(0)"), "999");
}
