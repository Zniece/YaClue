//! 加工层的步骤 API:调用引擎的 StepsD'Full/StepsI'Full(.ys 步骤生成器),
//! 提取结构化步骤(规则名 + 表达式 + 文案 + LaTeX)供 GUI 渲染。
//!
//! 步骤数据格式(来自 StepsX'Full):{规则名, 表达式, 文案} 三元组列表;
//! 本模块将其转为 `Step { rule, expr, why, tex }`。

use crate::engine::{Engine, EngineError, Expr};
use crate::quadrature::{adaptive_simpson, QuadratureOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepImportance {
    Routine,
    Normal,
    Key,
}

/// 一步:规则名 + 表达式 + 文案(声明式)+ LaTeX(GUI 渲染用)
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    /// 规则名(英文键,文案命中失败时前端回退显示它)
    pub rule: String,
    /// 表达式(yacas 形式)
    pub expr: String,
    /// 声明式文案(来自 Steps'Explain;空串 = 未登记键,前端回退)
    pub why: String,
    /// LaTeX(已去 $...$ 包裹,直接喂 KaTeX)
    pub tex: String,
    /// 全局粒度筛选使用的语义重要度。
    pub importance: StepImportance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepVerbosity {
    Concise,
    Standard,
    Detailed,
}

/// 校验用户表达式输入:必须是单个表达式,拒绝可注入引擎的字符。
/// 防护目标:命令注入(`;`/换行/`:=`/引号)、协议破坏(换行)、括号不闭合。
fn validate_expr(expr: &str) -> Result<(), EngineError> {
    let e = expr.trim();
    if e.is_empty() {
        return Err(EngineError::Eval("表达式为空".into()));
    }
    for bad in [';', '\n', '\r', ':', '"'] {
        if e.contains(bad) {
            return Err(EngineError::Eval(format!(
                "表达式包含不允许的字符 '{bad}'"
            )));
        }
    }
    // 括号平衡
    let open = e.chars().filter(|&c| c == '(').count();
    let close = e.chars().filter(|&c| c == ')').count();
    if open != close {
        return Err(EngineError::Eval("表达式括号不匹配".into()));
    }
    Ok(())
}

/// 执行 StepsX'Full 命令并提取步骤(规则名 + 表达式 + 文案 + LaTeX)
fn steps_from_command(
    engine: &mut dyn Engine,
    command: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    let r = engine.eval(command)?;
    let mut candidates = Vec::new();

    if let Expr::Call { head, args } = &r.expr {
        if head == "List" {
            let step_count = args.len();
            for (index, step) in args.iter().enumerate() {
                if let Expr::Call { args: pair, .. } = step {
                    if pair.len() >= 2 {
                        let rule = match &pair[0] {
                            Expr::Symbol(s) => s.trim_matches('"').to_string(),
                            other => other.to_string(),
                        };
                        let expr_str = pair[1].to_string();
                        // 四元组 {规则,表达式,文案,重要度}。
                        let why = match pair.get(2) {
                            Some(Expr::Symbol(s)) => s.trim_matches('"').to_string(),
                            Some(other) => other.to_string(),
                            None => String::new(),
                        };
                        let importance = match pair.get(3) {
                            Some(Expr::Number(value)) if value == "0" => StepImportance::Routine,
                            Some(Expr::Number(value)) if value == "2" => StepImportance::Key,
                            _ => StepImportance::Normal,
                        };
                        let keep = index + 1 == step_count
                            || match verbosity {
                                StepVerbosity::Detailed => true,
                                StepVerbosity::Standard => importance != StepImportance::Routine,
                                StepVerbosity::Concise => importance == StepImportance::Key,
                            };
                        if !keep {
                            continue;
                        }
                        candidates.push((rule, expr_str, why, importance));
                    }
                }
            }
        }
    }
    if candidates.is_empty() {
        return Err(EngineError::Eval(format!(
            "未能生成步骤(表达式可能不受支持): {command}"
        )));
    }
    let expressions: Vec<_> = candidates
        .iter()
        .map(|(_, expression, _, _)| expression.clone())
        .collect();
    let tex = engine.render_tex_batch(&expressions)?;
    if tex.len() != candidates.len() {
        return Err(EngineError::Parse("批量 TeX 结果数量与步骤数量不一致".into()));
    }
    Ok(candidates
        .into_iter()
        .zip(tex)
        .map(|((rule, expr, why, importance), tex)| Step {
            rule,
            expr,
            why,
            tex: strip_dollars(&tex),
            importance,
        })
        .collect())
}

/// 对 `expr` 关于 `var` 生成分步求导过程
pub fn derive_steps(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_steps_with_verbosity(engine, expr, var, StepVerbosity::Detailed)
}

pub fn derive_steps_with_verbosity(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expr(expr)?;
    steps_from_command(engine, &format!("StepsD'Full({expr}, {var})"), verbosity)
}

/// 对 `expr` 关于 `var` 生成 `order` 阶分步求导过程(步骤按求导轮次拼接)
pub fn derive_steps_order(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    order: u32,
) -> Result<Vec<Step>, EngineError> {
    derive_steps_order_with_verbosity(engine, expr, var, order, StepVerbosity::Detailed)
}

pub fn derive_steps_order_with_verbosity(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    order: u32,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expr(expr)?;
    if order == 0 {
        return Err(EngineError::Eval("求导阶数必须 >= 1".into()));
    }
    steps_from_command(engine, &format!("StepsD'Full({expr}, {var}, {order})"), verbosity)
}

/// 对 `expr` 关于 `var` 生成分步积分过程
pub fn derive_integrals(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_integrals_with_verbosity(engine, expr, var, StepVerbosity::Detailed)
}

pub fn derive_integrals_with_verbosity(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expr(expr)?;
    steps_from_command(engine, &format!("StepsI'Full({expr}, {var})"), verbosity)
}

/// 定积分:不定积分步骤链 + 牛顿-莱布尼茨求值(上下限可为任意表达式,如 `Pi`)
pub fn derive_definite(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    from: &str,
    to: &str,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_with_options(engine, expr, var, from, to, &QuadratureOptions::default())
}

pub fn derive_definite_with_verbosity(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    from: &str,
    to: &str,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(
        engine,
        expr,
        var,
        from,
        to,
        &QuadratureOptions::default(),
        verbosity,
    )
}

pub fn derive_definite_with_options(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    from: &str,
    to: &str,
    options: &QuadratureOptions,
) -> Result<Vec<Step>, EngineError> {
    derive_definite_configured(engine, expr, var, from, to, options, StepVerbosity::Detailed)
}

fn derive_definite_configured(
    engine: &mut dyn Engine,
    expr: &str,
    var: &str,
    from: &str,
    to: &str,
    options: &QuadratureOptions,
    verbosity: StepVerbosity,
) -> Result<Vec<Step>, EngineError> {
    validate_expr(expr)?;
    validate_expr(from)?;
    validate_expr(to)?;
    let mut steps = steps_from_command(
        engine,
        &format!("StepsI'Def'Full({expr}, {var}, {from}, {to})"),
        verbosity,
    )?;
    if steps.last().is_some_and(|step| step.rule == "direct") {
        let lower = numeric_scalar(engine, from, "下限")?;
        let upper = numeric_scalar(engine, to, "上限")?;
        let result = adaptive_simpson(engine, expr, var, (lower, upper), options)?;
        let value = format_number(result.value);
        let tex = engine
            .eval(&value)
            .map(|result| strip_dollars(&result.tex))
            .unwrap_or_else(|_| value.clone());
        steps.push(Step {
            rule: "numeric-integration-rule".into(),
            expr: value,
            why: format!(
                "自适应辛普森数值积分（估计误差 {:.2e}，{} 次采样）",
                result.estimated_error, result.evaluations
            ),
            tex,
            importance: StepImportance::Key,
        });
    }
    Ok(steps)
}

fn numeric_scalar(engine: &mut dyn Engine, expression: &str, label: &str) -> Result<f64, EngineError> {
    let result = engine.eval(&format!("N({expression})"))?;
    match result.expr {
        Expr::Number(value) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| EngineError::Eval(format!("定积分{label}不是有限实数: {expression}"))),
        _ => Err(EngineError::Eval(format!("定积分{label}不是有限实数: {expression}"))),
    }
}

fn format_number(value: f64) -> String {
    let text = format!("{value:.15}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 去掉 TeXForm 输出的首尾各一个 `$`(只剥一对,不用 trim_matches)
fn strip_dollars(tex: &str) -> String {
    let t = tex.trim();
    if t.len() >= 2 && t.starts_with('$') && t.ends_with('$') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EvalResult, ReplEngine, RustEngine};

    struct CountingEngine {
        inner: RustEngine,
        eval_calls: usize,
        batch_sizes: Vec<usize>,
    }

    impl Engine for CountingEngine {
        fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
            self.eval_calls += 1;
            self.inner.eval(command)
        }

        fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
            self.batch_sizes.push(expressions.len());
            self.inner.render_tex_batch(expressions)
        }
    }

    #[test]
    fn verbosity_filters_before_rendering_visible_steps() {
        let mut engine = CountingEngine {
            inner: RustEngine::spawn().unwrap(),
            eval_calls: 0,
            batch_sizes: Vec::new(),
        };
        let detailed = derive_steps_with_verbosity(
            &mut engine,
            "x^2",
            "x",
            StepVerbosity::Detailed,
        )
        .unwrap();
        assert!(detailed.iter().all(|step| !step.tex.is_empty()));

        let standard = derive_steps_with_verbosity(
            &mut engine,
            "x^2",
            "x",
            StepVerbosity::Standard,
        )
        .unwrap();
        let concise = derive_steps_with_verbosity(
            &mut engine,
            "x^2",
            "x",
            StepVerbosity::Concise,
        )
        .unwrap();
        assert!(
            detailed.len() > standard.len(),
            "detailed={} standard={} concise={}",
            detailed.len(), standard.len(), concise.len()
        );
        assert!(standard.len() > concise.len());
        assert_eq!(engine.eval_calls, 3, "each derivation evaluates its step chain once");
        assert_eq!(engine.batch_sizes, vec![detailed.len(), standard.len(), concise.len()]);
        assert_eq!(detailed.last().unwrap().expr, concise.last().unwrap().expr);
    }

    #[test]
    fn derive_steps_returns_rules_and_tex() {
        if !crate::engine::cpp_reference_available() {
            eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
            return;
        }
        let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
        let steps = derive_steps(&mut engine, "Sin(x)^2", "x").expect("StepsD 失败");

        assert!(steps.len() >= 3, "步骤过少: {}", steps.len());
        // Sin(x)^2:外层幂的底是复合式 → 链式键
        assert_eq!(steps[0].rule, "power-chain-rule");
        assert!(steps.iter().any(|s| s.rule == "sin-rule"));
        assert_eq!(steps.last().unwrap().rule, "simplify");
        // 每步都有可渲染的 LaTeX 与声明式文案
        for s in &steps {
            assert!(!s.tex.is_empty(), "步骤缺少 TeX: {s:?}");
            assert!(!s.why.is_empty(), "步骤缺少文案: {s:?}");
        }
        // 文案内容抽查(声明式,非教学腔)
        assert_eq!(steps[0].why, "链式法则: (u^n)' = n*u^(n-1)*u'");
        // 最后一步与引擎 D 代数等价
        let diff = engine
            .eval("Simplify(StepsD(Sin(x)^2, x)[Length(StepsD(Sin(x)^2, x))][2] - D(x)Sin(x)^2)")
            .expect("验证求值失败");
        assert_eq!(diff.expr.to_string(), "0");
    }

    #[test]
    fn derive_steps_errors_on_bad_input() {
        if !crate::engine::cpp_reference_available() {
            eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
            return;
        }
        let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
        // 非法语法(D 的非法逗号形式)应报错而非静默
        let err = derive_steps(&mut engine, "D(x^2,x)", "x").unwrap_err();
        assert!(err.to_string().contains("错误"), "应报告错误: {err}");
    }

    #[test]
    fn derive_integrals_works() {
        if !crate::engine::cpp_reference_available() {
            eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
            return;
        }
        let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
        // 分部积分
        let steps = derive_integrals(&mut engine, "x*Sin(x)", "x").expect("StepsI 失败");
        // x*Sin(x):分部策略横幅为第一步
        assert_eq!(steps[0].rule, "method-parts");
        let diff = engine
            .eval("Simplify(StepsI(x*Sin(x), x)[Length(StepsI(x*Sin(x), x))][2] - (Sin(x)-x*Cos(x)))")
            .expect("验证求值失败");
        assert_eq!(diff.expr.to_string(), "0", "分部积分结果错误: {}", diff.expr);

        // u-substitution
        let steps = derive_integrals(&mut engine, "Sin(x^2)*2*x", "x").expect("StepsI 失败");
        assert!(steps.iter().any(|s| s.rule == "u-sub-rule"));
        // u-sub 步骤带换元文案
        let usub = steps.iter().find(|s| s.rule == "u-sub-rule").unwrap();
        assert_eq!(usub.why, "换元: u = g(x), du = g'(x) dx");
        let diff = engine
            .eval("Simplify(StepsI(Sin(x^2)*2*x, x)[Length(StepsI(Sin(x^2)*2*x, x))][2] - (-Cos(x^2)))")
            .expect("验证求值失败");
        assert_eq!(diff.expr.to_string(), "0", "u-sub 结果错误: {}", diff.expr);

        // 非法输入同样被校验拦截
        assert!(derive_integrals(&mut engine, "x); Echo(1); (x", "x").is_err());
    }

    #[test]
    fn derive_steps_rejects_injection() {
        if !crate::engine::cpp_reference_available() {
            eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
            return;
        }
        let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
        // 命令注入尝试:分号、换行、赋值、引号、括号不匹配
        for bad in [
            "x); Echo(\"pwned\"); (x",
            "x;\nEcho(1)",
            "a:=99; Sin(x)^2",
            "Sin(x",
            "",
            "   ",
        ] {
            assert!(
                derive_steps(&mut engine, bad, "x").is_err(),
                "应拒绝恶意输入: {bad:?}"
            );
        }
        // 合法输入不受影响
        assert!(derive_steps(&mut engine, "Sin(x)^2", "x").is_ok());
    }

    /// 高阶导数:derive_steps_order 逐轮拼接(RustEngine 路径,默认可运行)
    #[test]
    fn derive_steps_order_works() {
        let mut engine = RustEngine::spawn().expect("启动 RustEngine 失败");
        let steps =
            derive_steps_order(&mut engine, "x^4", "x", 2).expect("StepsD'Full(order) 失败");
        assert!(steps.len() >= 4, "步骤过少: {}", steps.len());
        // 末轮最终形态与引擎二阶导一致(4*x^3 -> 12*x^2)
        let last = steps.last().unwrap();
        assert!(last.expr.contains("12"), "二阶导数异常: {}", last.expr);
        for s in &steps {
            assert!(!s.why.is_empty(), "步骤缺少文案: {s:?}");
        }
        // order=0 拒绝
        assert!(derive_steps_order(&mut engine, "x^4", "x", 0).is_err());
    }

    #[test]
    fn definite_integral_falls_back_to_bounded_quadrature() {
        let mut engine = RustEngine::spawn().expect("启动 RustEngine 失败");
        let steps = derive_definite(&mut engine, "Sin(x)/Sqrt(4-x^2)", "x", "0", "1")
            .expect("数值定积分失败");
        let last = steps.last().unwrap();
        assert_eq!(last.rule, "numeric-integration-rule");
        let value: f64 = last.expr.parse().unwrap();
        assert!((value - 0.2458433897).abs() < 1e-8, "{value}");
        assert!(last.why.contains("估计误差"));
    }
}
