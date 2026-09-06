//! T4-3 验收(G5 G6 + R3 R4):internal_use 对齐 cyacas(先置 loaded、失败不重试);
//! Use→DefLoad 双登记不重载;解析器「表达式后 EOF」合法收尾。
//! 期望全部来自 cyacas 引文:InternalUse(standard.cpp:423-437)、GetUserFunction
//! (lispeval.cpp:14-36:先查已有定义,不触发懒装载)、InfixParser::Parse(EOF 收尾)。

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

fn boot(env: &mut Environment) {
    // cyacas boot 序(yacasinit.ys):stdopers → patterns.rep → deffunc.rep → standard。
    let d = format!("{}/", scripts_root());
    run(env, &format!("DefaultDirectory(\"{d}\")"));
    // T5-b:stdarith(cyacas console 序 standard 之后)—— 脚本算术规则链完整。
    for f in ["stdopers.ys", "patterns.rep/code.ys", "deffunc.rep/code.ys", "standard.ys", "stdarith.ys"] {
        run(env, &format!("Use(\"{f}\")"));
    }
}

#[test]
fn internal_use_marks_loaded_before_load_failure_no_retry() {
    // cyacas InternalUse(standard.cpp:423-437):SetLoaded 在 InternalLoad **前**;
    // 装载失败也置位 → 后续 Use 直接跳过(失败不重试,照 cyacas;GetUserFunction
    // 同样先摘挂,失败路径不反复触发)。
    let mut env = Environment::new();
    boot(&mut env);
    assert!(matches!(
        run_err(&mut env, "Use(\"no_such_file.ys\")"),
        YacasError::FileNotFound
    ));
    let def = env
        .def_files
        .map
        .get("no_such_file.ys")
        .expect("Use 已 get-or-create def 表项");
    assert!(def.is_loaded, "失败也置位(照 cyacas SetLoaded 在 InternalLoad 前)");
    // 二次 Use:已置位 → 跳过装载、不再报错(返回 True)
    assert_eq!(run(&mut env, "Use(\"no_such_file.ys\")"), "True");
}

#[test]
fn use_then_def_load_does_not_reload() {
    // cyacas boot 语义:standard.ys 先 Use(装载+def 表项 loaded),LoadPackages 再
    // DefLoad(\"standard.ys\")(packages.ys:64) —— Nth 已定义 → GetUserFunction 先命中
    // 定义、不触发懒装载。重载会撞 ArityAlreadyDefined(RuleBase(Nth) 重声明),故
    // Nth 探针正常输出即反证未重载。二次 DefLoad 同文件 → 首符号已登记即错(照
    // cyacas DoLoadDefFile:63-68)。
    let mut env = Environment::new();
    boot(&mut env);
    assert!(
        env.def_files.map.get("standard.ys").expect("Use 已建表项").is_loaded,
        "Use 装载后 def 表项已置 loaded"
    );
    assert_eq!(run(&mut env, "DefLoad(\"standard.ys\")"), "True", "Use 后 DefLoad 可登记(首登记)");
    assert!(matches!(
        run_err(&mut env, "DefLoad(\"standard.ys\")"),
        YacasError::DefFileAlreadyChosen
    ), "二次 DefLoad 同文件 → 首符号已登记即错(照 cyacas)");
    assert_eq!(run(&mut env, "Nth({a,b,c},2)"), "b", "Nth 探针正常=未重载(重载会 ArityAlreadyDefined)");
}

#[test]
fn parser_tolerates_eof_without_final_semicolon() {
    // cyacas InfixParser::Parse:表达式后到 EOF 合法收尾(R4;文件末语句无 `;` 仍装载;
    // 末语句有 `;` 时由下一轮 parse 的 EndOfFile 分支收尾)。Rust 曾对 lookahead 非
    // `;` 一律 fail —— 末语句无分号会误报。临时目录合成探针文件验证。
    let dir = std::env::temp_dir().join(format!("cas_r4_probe_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("r4probe.ys");
    std::fs::write(&file, "x:=1;\ny").expect("write probe"); // 末语句 `y` 无分号
    let mut env = Environment::new();
    boot(&mut env);
    run(&mut env, &format!("DefaultDirectory(\"{}/\")", dir.display()));
    assert_eq!(run(&mut env, "Use(\"r4probe.ys\")"), "True", "末语句无分号仍装载(照 cyacas)");
    assert_eq!(run(&mut env, "x"), "1", "装载内容生效");
    std::fs::remove_dir_all(&dir).ok();
}