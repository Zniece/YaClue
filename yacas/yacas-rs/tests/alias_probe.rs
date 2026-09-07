//! 别名/共享链语义锁步(Step 4;oracle 锚点逐条钉死,historical porting notes):
//! cyacas LispObject 为可变链表,Copy 只复制 SubList 盒子、内容链共享 ——
//! 破坏性命令就地改链 → 所有共享槽同时可见。Rust 重建模型经 propagate_alias
//! 三层广播模拟(根层/容器层/深度包含),不做被替换元素级广播(会污染交换)。
use yacas_rs::env::Environment;
use yacas_rs::evaluator::eval;
use yacas_rs::parser::parse_expression;
use yacas_rs::printer::infix_print;

fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap().expect("非空");
    match eval(env, &t) {
        Ok(r) => infix_print(env, &r),
        Err(e) => format!("ERR({e:?})"),
    }
}

#[test]
fn alias_lockstep() {
    let mut env = Environment::new();
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/../../yacas/scripts/");
    run(&mut env, &format!("DefaultDirectory(\"{d}\")"));
    assert_eq!(run(&mut env, "Load(\"yacasinit.ys\")"), "True");
    // (probe, oracle 期望)—— 全部锚点逐条 oracle 实测(2026-09 Step 3 期间);
    // 多语句序列用 Prog 块 [..;..;last;](块内 ';' 终止符,§30)。
    let cases: [(&str, &str); 7] = [
        // 根层别名:b:=a 后 b[1]:=9,a[1] 同变(deep_assign 根层广播)
        ("[Local(a,b); a:={1,2}; b:=a; b[1]:=9; a[1];]", "9"),
        // 隔代深度包含:it:=l[1]; it[2]:=-7 → l 内层同步(deep_replace)
        ("[Local(l,it); l:={{1,5}}; it:=l[1]; it[2]:=-7; l;]", "{{1,-7}}"),
        // 交换不受元素级广播污染(不做被替换元素广播;SmallSort 依赖)
        ("[Local(l,t); l:={{2,3},{5,1},{3,2}}; t:=l[1]; l[1]:=l[2]; l[2]:=t; l;]",
         "{{5,1},{2,3},{3,2}}"),
        // 写回后别名读一致
        ("[Local(a,b); a:={1,2}; b:=a; b[1]:=9; b[1];]", "9"),
        // 容器层:深层写中间容器,根同步(oracle:l[1][2][1]:=99 → {{1,{99,3}}})
        ("[Local(l); l:={{1,{2,3}}}; l[1][2][1]:=99; l;]", "{{1,{99,3}}}"),
        // MathNth 读出的共享拷贝不受后续写影响(l[1]:=v 只改一个链节)
        ("[Local(l,t); l:={1,2}; t:=l[1]; l[1]:=9; t;]", "1"),
        // Mod/Apart 电池(Step 2 验收锚点,随别名套件常驻)
        ("Mod(7,3)", "1"),
    ];
    let mut fails = 0;
    for (p, want) in cases {
        let got = run(&mut env, p);
        let ok = got == want;
        if !ok { fails += 1; }
        println!("[{}] {} => {got} (期望 {want})", if ok { "OK" } else { "DIFF" }, p);
    }
    assert_eq!(fails, 0, "{fails} 条别名锚点未对齐");
}
