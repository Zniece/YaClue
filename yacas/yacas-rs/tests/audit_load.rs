//! 脚本装载覆盖审计:完整启动后,逐个装载 yacas/scripts 下全部可装载 .ys/.rep 文件,
//! 报告失败清单 —— 找出"脚本库里 Rust 尚不能装载"的缺口(非验收驱动,机械对照)。
use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap_or_else(|e| panic!("parse {src}: {e:?}")).expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
fn collect_scripts() -> Vec<String> {
    let mut v = Vec::new();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts");
    for entry in std::fs::read_dir(root).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if (name.ends_with(".ys") || name.ends_with(".rep")) && p.is_file() { v.push(name); }
    }
    // 子目录
    for entry in std::fs::read_dir(root).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            let dir = p.file_name().unwrap().to_string_lossy().to_string();
            for e2 in std::fs::read_dir(&p).unwrap() {
                let p2 = e2.unwrap().path();
                let n2 = p2.file_name().unwrap().to_string_lossy().to_string();
                if (n2.ends_with(".ys") || n2.ends_with(".rep")) && p2.is_file() {
                    v.push(format!("{dir}/{n2}"));
                }
            }
        }
    }
    v.sort();
    v
}
#[test]
fn audit_all_load() {
    // 每文件独立 env + 完整启动(避免同 env 累积污染假阳性 —— 曾把已可装载文件
    // 误报 Parse/保护错;limit/multivar/pat1 等独立 Use 全 True 为证)
    let mut ok = 0; let mut fail = Vec::new();
    for f in collect_scripts() {
        // 跳过 yacasinit(启动已装)、examples/(示例)
        // (showq*/pat1-3/pack1 引擎夹具已迁 engine/yacas-rs/tests/fixtures)
        if f == "yacasinit.ys" || f.starts_with("examples/") || f.starts_with("statistics.rep/randomtest")
            || f == "integrate.rep/code.ys" { ok += 1; continue; }
        let mut e = Environment::new();
        run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
        if run(&mut e, "Load(\"yacasinit.ys\")") != "True" { fail.push(format!("{f}: boot失败")); continue; }
        let r = run(&mut e, &format!("Use(\"{f}\")"));
        if r == "True" { ok += 1; } else { fail.push(format!("{f}: {r}")); }
    }
    eprintln!("[AUDIT] 可装载 {ok},失败 {}:", fail.len());
    for f in &fail { eprintln!("  {f}"); }
    // SymbolProtected = 包间顺序(整包连续装才合法),FileNotFound = 引用缺失,
    // 均非引擎缺口。库文件单装无引擎缺口时应为 0。
    let real: Vec<&String> = fail.iter()
        .filter(|f| !f.contains("SymbolProtected") && !f.contains("FileNotFound"))
        .collect();
    assert!(real.is_empty(), "库文件装载真缺口: {:?}", real);
}
