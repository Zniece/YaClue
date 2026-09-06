//! T4-1 验收(G1 G2 G3 G7):Use/Load/Hold/DefaultDirectory 命令 + 目录搜索链 +
//! DefLoadFunction 只摘挂语义(cyacas 实测为期望,零手写)。

use yacas_rs::env::Environment;
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

fn scripts_root() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts").to_string()
}

/// 引擎语义测试夹具(pack1/pat1-3/show* 等)已从 yacas/scripts 迁出
fn fixtures_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures").to_string()
}

#[test]
fn default_directory_seeds_input_dirs() {
    // cyacas 启动 = 主程序把 rootdir 逐段 DefaultDirectory(...)(yacasmain.cpp:
    // 368-390;LispDefaultDirectory push_back,mathcommands.cpp:1106-1114)。
    let mut env = Environment::new();
    let d = format!("{}/", scripts_root());
    assert_eq!(run(&mut env, &format!("DefaultDirectory(\"{d}\")")), "True");
    assert_eq!(env.input_directories, vec![d]);
}

#[test]
fn use_loads_boot_files_via_directory_search() {
    // cyacas 实测链:Use 依次载 stdopers/patterns.rep/code.ys/deffunc.rep/code.ys/
    // standard.ys(目录搜索解析相对名,照 InternalFindFile:CWD→iInputDirectories)。
    // 装载后 Nth/NrArgs 可调(standard.ys 的 10# 规则;对照 load_standard.rs 期望)。
    let mut env = Environment::new();
    run(&mut env, &format!("DefaultDirectory(\"{}/\")", scripts_root()));
    assert_eq!(run(&mut env, "Use(\"stdopers.ys\")"), "True");
    assert_eq!(run(&mut env, "Use(\"patterns.rep/code.ys\")"), "True");
    assert_eq!(run(&mut env, "Use(\"deffunc.rep/code.ys\")"), "True");
    assert_eq!(run(&mut env, "Use(\"standard.ys\")"), "True");
    // T5-b:stdarith(cyacas console 顺序 standard 之后)—— `+ - * / ^` 脚本规则所在。
    assert_eq!(run(&mut env, "Use(\"stdarith.ys\")"), "True");
    assert_eq!(run(&mut env, "Nth({a,b,c},2)"), "b");
    assert_eq!(run(&mut env, "Nth({a,b,c},3)"), "c");
    assert_eq!(run(&mut env, "NrArgs(f(a,b,c))"), "3");
}

#[test]
fn load_does_not_touch_def_registry() {
    // cyacas:Load = InternalLoad(读-解析-求值),不查/不建 def 登记表(G1 语义区分)。
    // pack1.ys 定义 Probe1(x):=2*x → Load 后 Probe1(3)→6;且 def 表无 "pack1.ys"
    // 条目(对照:Use 会建)。G2 目录搜索同时被验证。
    // 注:`:=` 系 deffunc 脚本机制(console 启动链早已载好);裸 env 需先 Use 三步
    // 与 cyacas boot 状态一致,pack1.ys 的 `Probe1(x) := …` 才可定义。
    let mut env = Environment::new();
    run(&mut env, &format!("DefaultDirectory(\"{}/\" )", scripts_root()));
    run(&mut env, &format!("DefaultDirectory(\"{}/\")", fixtures_dir()));
    run(&mut env, "Use(\"stdopers.ys\")");
    run(&mut env, "Use(\"patterns.rep/code.ys\")");
    run(&mut env, "Use(\"deffunc.rep/code.ys\")");
    // cyacas console 顺序:deffunc 之后紧接 standard.ys(yacasinit.ys:44)——
    // `:=` 函数定义体用 aLeftAssign[0](Nth,standard.ys 定义),缺 Nth 则 String(子列表)
    // InvalidArg(实测定位);stdarith 再后(45)—— `2*x` 脚本 * 规则(Probe1 体调用)。
    run(&mut env, "Use(\"standard.ys\")");
    run(&mut env, "Use(\"stdarith.ys\")");
    assert_eq!(run(&mut env, "Load(\"pack1.ys\")"), "True");
    assert_eq!(run(&mut env, "Probe1(3)"), "6");
    assert_eq!(run(&mut env, "Loaded'Probe1"), "True");
    assert!(
        !env.def_files.map.contains_key("pack1.ys"),
        "Load 不应建 def 登记表项(那是 Use/DefLoad 的职责)"
    );
    let name = yacas_rs::standard::symbol_name(&mut env, "Probe1");
    let entry = env.user_functions.get(&name).expect("Probe1 已定义");
    assert!(
        entry.inner.borrow().file_to_open.is_none(),
        "Load 直载函数无懒挂点(未 DefLoad 登记)"
    );
}

#[test]
fn hold_returns_argument_unevaluated() {
    // cyacas:Hold = ARGUMENT(1) 复制返回(实参保持,不求值);G3 —— Defun 体
    // Set(fn,Hold(@func)) 依赖,否则 fn 绑成非原子 → Rule(@fn,…) 报错。
    // 若误实现为"求值",Hold(1+2) 会得 "3"(cyacas 实测得 "1+2")。
    let mut env = Environment::new();
    assert_eq!(run(&mut env, "Hold(1+2)"), "1+2");
    assert_eq!(run(&mut env, "Hold(aa+bb)"), "aa+bb");
    let r = run(&mut env, "Hold({a,b})");
    assert_eq!(r, "{a,b}");
}

#[test]
fn def_load_function_clears_hook_without_loading() {
    // cyacas(行为权威,mathcommands.cpp:1983-2002):DefLoadFunction 只摘挂不加载。
    // 未登记名 → MultiUserFunction get-or-create(空)+iFileToOpen 本就 null →
    // 无装载、返回 True(G7 翻转自 Java 立即 InternalUse 版;立即装载的探针反证
    // 见 pack1 系在 T4-2,G4 实装后补真挂点用例)。
    let mut env = Environment::new();
    assert_eq!(run(&mut env, "DefLoadFunction(\"Probe99\")"), "True");
    let name = yacas_rs::standard::symbol_name(&mut env, "Probe99");
    assert!(
        env.user_functions.contains_key(&name),
        "cyacas 语义:未登记名也建空条目"
    );
}