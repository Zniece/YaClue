//! T4-2 验收(G4 + R1 R2):cmd_def_load 实装 cyacas DoLoadDefFile 语义。
//! 期望全部来自 cyacas 引文:LoadDefFile(deffile.cpp:75-92:flatfile=unstringify(name)
//! +".def",目录链打开,失败 LispErrFileNotFound)、DoLoadDefFile(deffile.cpp:33-73:
//! token 读到 `}` 或 EOF;每符号 get-or-create+file_to_open+symbols.insert+Protect;
//! 符号已被登记 → 打 `[sym]` 后抛 DefFileAlreadyChosen)。懒触发在首调用
//! (GetUserFunction,lispeval.cpp:14-36:摘挂→InternalUse) —— Rust 侧由
//! evaluator::get_user_function 承担,本组探针验证全链。

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

fn run_err(env: &mut Environment, src: &str) -> YacasError {
    let tree = parse_expression(env, &format!("{src};"))
        .unwrap()
        .expect("非空");
    match eval(env, &tree) {
        Err(e) => e,
        Ok(_) => panic!("期望 {src} 报错,实际成功"),
    }
}

fn scripts_root() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts").to_string()
}

/// 引擎语义测试夹具(pack1 等)已从 yacas/scripts 迁出
fn fixtures_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures").to_string()
}

fn boot(env: &mut Environment) {
    // cyacas boot 序(yacasinit.ys):stdopers → patterns.rep → deffunc.rep → standard。
    // `:=` 函数定义体依赖 Nth(standard.ys)—— T4-1 拆句探针实测定位。
    let d = format!("{}/", scripts_root());
    run(env, &format!("DefaultDirectory(\"{d}\")"));
    // 夹具目录(pack1.ys 等经 DefLoad 搜索)
    let f = format!("{}/", fixtures_dir());
    run(env, &format!("DefaultDirectory(\"{f}\")"));
    // T5-b:stdarith(cyacas console 序 standard 之后)—— Probe1 体 `2*x` 脚本 * 规则。
    for f in ["stdopers.ys", "patterns.rep/code.ys", "deffunc.rep/code.ys", "standard.ys", "stdarith.ys"] {
        run(env, &format!("Use(\"{f}\")"));
    }
}

#[test]
fn def_load_missing_def_file_errors() {
    // cyacas:LocalFile 打开 flatfile 失败 → LispErrFileNotFound(deffile.cpp:85-86)。
    let mut env = Environment::new();
    boot(&mut env);
    assert!(matches!(
        run_err(&mut env, "DefLoad(\"no_such_pkg.ys\")"),
        YacasError::FileNotFound
    ));
}

#[test]
fn def_load_claims_symbols_then_first_call_triggers_load() {
    // cyacas 实测链:DefLoad(\"pack1.ys\") 只登记(Probe1 挂 pack1.ys + Protect,
    // def 表项未装载);首调用 Probe1 → 摘挂+InternalUse(\"pack1.ys\") → 装载
    // (pack1.ys 的 `Probe1(x) := 2*x` 定义)→ 求值 → 8。装载后符号重 Protect。
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "DefLoad(\"pack1.ys\")"), "True");
    // 登记态:未装载、Probe1 挂点已设、符号已 Protect
    assert!(
        !env.def_files.map.get("pack1.ys").expect("def 表项已建").is_loaded,
        "DefLoad 只登记不装载(cyacas 实测)"
    );
    let name = yacas_rs::standard::symbol_name(&mut env, "Probe1");
    assert!(env.is_protected(&name), "登记即 Protect(照 DoLoadDefFile:70)");
    let entry = env.user_functions.get(&name).expect("Probe1 条目 get-or-create");
    assert!(
        entry.inner.borrow().file_to_open.is_some(),
        "挂点已设(首调用懒触发用)"
    );
    // 首调用触发装载(GetUserFunction 语义)
    assert_eq!(run(&mut env, "Probe1(4)"), "8");
    assert!(
        env.def_files.map.get("pack1.ys").unwrap().is_loaded,
        "首调用已触发装载"
    );
    assert!(env.is_protected(&name), "装载后重 Protect(照 InternalUse,standard.cpp:434-435)");
}

#[test]
fn def_load_duplicate_claim_errors() {
    // cyacas:同一符号二次登记(file_to_open 非空)→ DefFileAlreadyChosen(deffile.cpp:63-68)。
    // 同一文件二次 DefLoad 即触发(重读清单、首符号已登记)。
    let mut env = Environment::new();
    boot(&mut env);
    run(&mut env, "DefLoad(\"pack1.ys\")");
    assert!(matches!(
        run_err(&mut env, "DefLoad(\"pack1.ys\")"),
        YacasError::DefFileAlreadyChosen
    ));
}

#[test]
fn claimed_symbol_protected_until_loaded() {
    // 照 DoLoadDefFile:70 登记即 Protect;装载前 Set(Probe1,…) → SymbolProtected
    // (cyacas SetVariable/DefineRule 保护检查)。
    let mut env = Environment::new();
    boot(&mut env);
    run(&mut env, "DefLoad(\"pack1.ys\")");
    assert!(matches!(
        run_err(&mut env, "Set(Probe1, 99)"),
        YacasError::SymbolProtected
    ));
}

#[test]
fn failed_lazy_load_restores_symbol_protection() {
    let mut env = Environment::new();
    boot(&mut env);
    assert_eq!(run(&mut env, "DefLoad(\"broken_lazy.ys\")"), "True");
    let name = yacas_rs::standard::symbol_name(&mut env, "BrokenLazy");
    assert!(env.is_protected(&name));

    assert!(matches!(
        run_err(&mut env, "BrokenLazy()"),
        YacasError::FileNotFound
    ));
    assert!(
        env.is_protected(&name),
        "a failed lazy load must not leak its temporary unprotected state"
    );
    assert_eq!(run(&mut env, "2+2"), "4", "the environment remains usable");
}
