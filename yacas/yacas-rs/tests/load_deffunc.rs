//! 层 7 T2 验收:deffunc.rep/code.ys 脚本装载(Function/Macro/`:=` 脚本规则生效)
//! + DestructiveDelete/Eval/MacroSet/Delete/BackQuote 命令实装。
//!
//! 断言方向照契约 §7(行为锁 cyacas,期望值全部 cyacas 普通模式实测,零手写):
//! - Function(`f` 3 参) = **分支**函数(照 C++ MacroRuleBase→InternalRuleBase→DeclareRuleBase);
//! - Macro(`m` 3 参) = **宏**(DefMacroRuleBase):**不绑形参局部**(m5(3)→y、p1(3)→99 读全局),
//!   @ 替换取**位置实参**(m6(3)→10、q(3)→3);
//! - `<--`/`#` 模式规则(G/V/T)绑定发生在规则匹配期(继续走 6b-7 锁);
//! - `:=` 由脚本规则接管(原子:MacroSet+Eval 链;数字名 → InvalidArg 变体);
//! - Delete 1 基、越界 ListNotLongEnough;DestructiveDelete 写回 arg0 变量(cc 本体变 {1,3});
//! - IsAtom:数字/字符串 = True(照 cyacas 实测,非仅原子串)。
//! 注:`{aa,bb}:={5,6}` 列表赋值依赖 Map(lists.rep 包脚本) → 归 T5,不在本批。

use yacas_rs::env::Environment;
use yacas_rs::errors::YacasError;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap_or_else(|e| panic!("parse {src}: {e:?}"))
        .expect("非空");
    let result = eval(env, &tree).unwrap_or_else(|e| panic!("eval {src}: {e:?}"));
    infix_print(env, &result)
}

/// T2 装载序(照 cyacas 启动链:code.ys → deffunc.rep/code.ys;Nth 暂用 bootstrap_nth,
/// standard.ys 装载后(T3)由脚本接管)。
fn boot(env: &mut Environment) {
    let code_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/patterns.rep/code.ys"
    );
    yacas_rs::standard::internal_load(env, code_ys).unwrap();
    let deffunc_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/deffunc.rep/code.ys"
    );
    yacas_rs::standard::internal_load(env, deffunc_ys).unwrap();
    // 装载序照 yacasinit.ys:44-45(standard.ys → stdarith.ys);stdarith 依赖 standard
    // 的 Nth/规则骨架,反序装载会 InvalidArg(T5 曾误注"cyacas console 序"反序)。
    let standard_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/standard.ys"
    );
    yacas_rs::standard::internal_load(env, standard_ys).unwrap();
    // stdarith —— 宏体 `@y+7` / `@qq+0` 的脚本加法规则(cyacas console 序)。
    let stdarith_ys = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../yacas/scripts/stdarith.ys"
    );
    yacas_rs::standard::internal_load(env, stdarith_ys).unwrap();
}

fn err_of(env: &mut Environment, src: &str) -> YacasError {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("非空");
    match eval(env, &tree) {
        Err(e) => e,
        Ok(_) => panic!("期望 {src} 报错,实际成功"),
    }
}

#[test]
fn function_is_branching_like_cyacas() {
    // cyacas 实测:Function(f,{x})(x+x) → True;f(3) → 6;aa:=5;Function(f2,{x})(x+x);
    // f2(aa) → 5 / True / 10(分支函数:实参先求值再绑形参)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "Function(f,{x})(x+x)"), "True");
    assert_eq!(run(&mut env, "f(3)"), "6");
    assert_eq!(run(&mut env, "aa := 5"), "5");
    assert_eq!(run(&mut env, "Function(f2,{x})(x+x)"), "True");
    assert_eq!(run(&mut env, "f2(aa)"), "10");
}

#[test]
fn macro_plain_rules_do_not_bind_params() {
    // cyacas 实测(宏=DefMacroRuleBase):形参不绑定 —— m5(3)→y(无全局 y)、
    // y:=99;p1(3)→99(读全局)、x:=10;m4(3)→20(读全局 x)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "Macro(m5,{y})(y)"), "True");
    assert_eq!(run(&mut env, "m5(3)"), "y");
    assert_eq!(run(&mut env, "y := 99"), "99");
    assert_eq!(run(&mut env, "Macro(p1,{y})(y)"), "True");
    assert_eq!(run(&mut env, "p1(3)"), "99");
    assert_eq!(run(&mut env, "x := 10"), "10");
    assert_eq!(run(&mut env, "Macro(m4,{x})(x+x)"), "True");
    assert_eq!(run(&mut env, "m4(3)"), "20");
}

#[test]
fn macro_at_subst_is_positional() {
    // cyacas 实测(@ 替换取位置实参,非变量读取):无全局 y 时 m6(3)→10(@y→3);
    // qq:=99 时 q(3)→3(@qq→实参 3,非全局 99)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "Macro(m6,{y})(@y+7)"), "True");
    assert_eq!(run(&mut env, "m6(3)"), "10");
    assert_eq!(run(&mut env, "qq := 99"), "99");
    assert_eq!(run(&mut env, "Macro(q,{qq})(@qq+0)"), "True");
    assert_eq!(run(&mut env, "q(3)"), "3");
}

#[test]
fn set_assign_and_eval() {
    // cyacas 实测:aa:=5 → 5;aa → 5;Eval(aa) → 5(`:=` 原子规则:MacroSet+Eval 链)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "aa := 5"), "5");
    assert_eq!(run(&mut env, "aa"), "5");
    assert_eq!(run(&mut env, "Eval(aa)"), "5");
}

#[test]
fn delete_and_destructive_delete() {
    // cyacas 实测:Delete({1,2,3},1)→{2,3}、Delete({1,2,3},3)→{1,2};
    // Delete({1,2,3},5)→ListNotLongEnough 错;cc:={1,2,3};DestructiveDelete(cc,2)→{1,3}
    // 且 cc 本体变 {1,3}(破坏版写回)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "Delete({1,2,3},1)"), "{2,3}");
    assert_eq!(run(&mut env, "Delete({1,2,3},3)"), "{1,2}");
    assert!(matches!(
        err_of(&mut env, "Delete({1,2,3},5)"),
        YacasError::ListNotLongEnough
    ));
    assert_eq!(run(&mut env, "cc := {1,2,3}"), "{1,2,3}");
    assert_eq!(run(&mut env, "DestructiveDelete(cc,2)"), "{1,3}");
    assert_eq!(run(&mut env, "cc"), "{1,3}");
}

#[test]
fn assign_to_number_is_invalid_arg() {
    // cyacas 实测:`1:=2` → IsAtom(1)=True(数字算原子)→ 原子规则 → MacroSet 内
    // CheckArg(!IsNumber) 失败 → "In function \"MacroSet\" : Invalid argument"(变体 InvalidArg)。
    let mut env = Environment::new();
    boot(&mut env);
    assert!(matches!(err_of(&mut env, "1 := 2"), YacasError::InvalidArg));
}

#[test]
fn is_atom_matches_cyacas() {
    // cyacas 实测:IsAtom(1)=True、IsAtom(2.5)=True、IsAtom(foo)=True、IsAtom("s")=True、
    // IsAtom({1})=False —— 数字/字符串/普通原子均 True,子列表 False。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "IsAtom(1)"), "True");
    assert_eq!(run(&mut env, "IsAtom(2.5)"), "True");
    assert_eq!(run(&mut env, "IsAtom(foo)"), "True");
    assert_eq!(run(&mut env, "IsAtom(\"s\")"), "True");
    assert_eq!(run(&mut env, "IsAtom({1})"), "False");
}

#[test]
fn pattern_macro_path_stays_locked() {
    // cyacas 实测(6b-7 锁,复测):V(_x)<--x → V(7)=7、V(f(aa))=f(aa)(单原子保持不重求值);
    // T(_x)<--x+x → T(3)=6;10#f(_x)<--2*x 后 G(_x,_y)<--x+y → G(f(aa),bb)=2*aa+bb
    // (模式宏:绑定在规则匹配期;数学命令保持路径对保持树再求值)。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "V(_x) <-- x"), "True");
    assert_eq!(run(&mut env, "V(7)"), "7");
    assert_eq!(run(&mut env, "V(f(aa))"), "f(aa)");
    assert_eq!(run(&mut env, "T(_x) <-- x+x"), "True");
    assert_eq!(run(&mut env, "T(3)"), "6");
    assert_eq!(run(&mut env, "10 # f(_x) <-- 2*x"), "True");
    assert_eq!(run(&mut env, "G(_x,_y) <-- x+y"), "True");
    assert_eq!(run(&mut env, "G(f(aa),bb)"), "2*aa+bb");
}