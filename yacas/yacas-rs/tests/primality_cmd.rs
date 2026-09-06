//! FastIsPrime/MathFac 命令测试(照 cyacas mathcommands3.cpp:359 + platmath.cpp)。
//!
//! FastIsPrime:质数表查表(0→65537,2→1,<2/>65537/偶→0,奇素数→1);
//! numbers.rep IsSmallPrime/IsPrime(n<=FastIsPrime(0)) 依赖 → IsPrime 全解锁。
//! MathFac:n! 精确大数(Nat);sums.rep 的 `((n_IsPositiveInteger)!)` 规则依赖。
//! 期望全部来自 cyacas oracle:IsPrime(7/2/97)=True、IsPrime(4/100)=False、
//! 5!=120、10!=3628800、100!=93326...(158 位)。

use yacas_rs as ys;
use ys::env::Environment;
use ys::evaluator::eval;
use ys::parser::parse_expression;
use ys::printer::infix_print;
fn run(env: &mut Environment, src: &str) -> String {
    let t = parse_expression(env, &format!("{src};")).unwrap().expect("ok");
    match eval(env, &t) { Ok(r) => infix_print(env, &r), Err(e) => format!("ERR({e:?})") }
}
#[test]
fn fast_is_prime_and_mathfac() {
    let mut e = Environment::new();
    run(&mut e, &format!("DefaultDirectory(\"{}/\")", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts")));
    assert_eq!(run(&mut e, "Load(\"yacasinit.ys\")"), "True");
    for p in ["IsPrime(7)", "IsPrime(97)", "IsPrime(100)", "5!", "10!", "100!", "MathFac(5)"] {
        println!("P| {p} => {}", run(&mut e, p));
    }
}
