use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::repl::{default_scripts_dir, default_steps_dir};
use super::rust::eval_cmd;
use super::*;

/// Keep a broken channel/timeout regression from hanging the whole suite.
fn finishes_promptly(work: impl FnOnce() + Send + 'static) {
    let (done_tx, done_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        work();
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("engine operation must complete within 10 seconds");
    worker.join().expect("engine test worker panicked");
}

#[test]
fn rust_eval_executes_input_once_and_preserves_held_values() {
    let mut engine = RustEngine::spawn().unwrap();
    engine.eval("reviewCounter:=0").unwrap();
    let result = engine.eval("reviewCounter:=reviewCounter+1").unwrap();
    assert_eq!(result.expr.to_string(), "1");
    assert_eq!(result.tex, "$1$");
    assert_eq!(engine.eval("reviewCounter").unwrap().expr.to_string(), "1");

    let held = engine.eval("Hold(reviewCounter:=reviewCounter+1)").unwrap();
    assert!(held.expr.to_string().contains("reviewCounter"));
    assert!(held.tex.contains("reviewCounter"));
    assert_eq!(engine.eval("reviewCounter").unwrap().expr.to_string(), "1");
    assert!(engine.env.eval_deadline.is_none());
}

#[test]
fn rust_engine_is_secure_after_startup() {
    let mut engine = RustEngine::spawn().unwrap();
    assert!(engine.env.secure);
    for command in [r#"SystemCall("true")"#, r#"Load("yacasinit.ys")"#] {
        let error = engine.eval(command).unwrap_err();
        assert!(matches!(error, EngineError::Eval(_)), "{error}");
    }
    assert_eq!(engine.eval("2+3").unwrap().expr, Expr::Number("5".into()));
}

#[test]
fn rust_batch_tex_matches_individual_evaluation() {
    let expressions = vec!["3*2*x".to_string(), "Sin(-4*x)".to_string()];
    let mut individual = RustEngine::spawn().unwrap();
    let expected: Vec<_> = expressions
        .iter()
        .map(|expression| individual.eval(expression).unwrap().tex)
        .collect();

    let mut batched = RustEngine::spawn().unwrap();
    assert_eq!(batched.render_tex_batch(&expressions).unwrap(), expected);
    assert!(batched.env.eval_deadline.is_none());
    assert!(matches!(
        batched.render_tex_batch(&["Sin(".into()]),
        Err(EngineError::Eval(_))
    ));
    assert!(batched.env.eval_deadline.is_none());
    assert_eq!(batched.eval("2+3").unwrap().tex, "$5$");
}

#[test]
fn rust_eval_recovers_after_parse_and_tex_errors() {
    let mut engine = RustEngine::spawn().unwrap();
    eval_cmd(
        &mut engine.env,
        "1 # TeXForm(ReviewBrokenTex) <-- Check(False, \"broken TeX\")",
    )
    .unwrap();
    assert_eq!(
        engine.eval_expr("ReviewBrokenTex").unwrap(),
        Expr::Symbol("ReviewBrokenTex".into())
    );
    for command in ["Sin(", "ReviewBrokenTex"] {
        let error = engine.eval(command).unwrap_err();
        assert!(matches!(error, EngineError::Eval(_)), "{error}");
        assert!(engine.env.eval_deadline.is_none());
        let result = engine.eval("2+3").expect("engine recovers after errors");
        assert_eq!(result.expr.to_string(), "5");
        assert_eq!(result.tex, "$5$");
    }
}

#[test]
fn rust_eval_times_out_in_command_and_tex_then_recovers() {
    finishes_promptly(|| {
        let mut engine = RustEngine::spawn().unwrap();
        eval_cmd(
            &mut engine.env,
            "1 # TeXForm(ReviewSlowTex) <-- While(True) 1",
        )
        .unwrap();
        for command in ["While(True) 1", "ReviewSlowTex"] {
            let error = engine
                .eval_with_timeout(command, Duration::from_millis(20))
                .unwrap_err();
            assert!(matches!(error, EngineError::Timeout(_)), "{error}");
            assert!(engine.env.eval_deadline.is_none());
            let result = engine
                .eval("2+3")
                .expect("engine recovers after interruption");
            assert_eq!(result.expr.to_string(), "5");
            assert_eq!(result.tex, "$5$");
        }
    });
}

#[test]
fn rust_proxy_rejects_bad_script_paths_during_spawn() {
    finishes_promptly(|| {
        // An existing file cannot be a script directory. No process-wide
        // environment changes, so this is safe alongside other tests.
        let not_a_directory = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        for (scripts, steps) in [
            (not_a_directory.to_string(), default_steps_dir()),
            (default_scripts_dir(), not_a_directory.to_string()),
        ] {
            for _ in 0..2 {
                let scripts = scripts.clone();
                let steps = steps.clone();
                let result = RustEngineProxy::spawn_with_initializer(move || {
                    RustEngine::spawn_with_scripts(scripts, steps)
                });
                assert!(matches!(result, Err(EngineError::Spawn(_))));
            }
        }
        let mut engine = RustEngineProxy::spawn().expect("valid startup still succeeds");
        assert_eq!(engine.eval("2+3").unwrap().tex, "$5$");
        drop(engine); // Also check worker shutdown after a successful request.
    });
}

#[test]
fn rust_proxy_recovers_after_request_error_and_shuts_down() {
    finishes_promptly(|| {
        let mut engine = RustEngineProxy::spawn().unwrap();
        assert!(matches!(engine.eval("Sin("), Err(EngineError::Eval(_))));
        for command in ["1+1", "2"] {
            assert_eq!(engine.eval(command).unwrap().tex, "$2$");
        }
        assert_eq!(engine.eval_expr("2+3").unwrap(), Expr::Number("5".into()));
        assert_eq!(
            engine
                .render_tex_batch(&["2+3".into(), "Sin(x)".into()])
                .unwrap(),
            vec!["$5$", "$\\sin x$"]
        );
        drop(engine);
    });
}

#[test]
fn rust_proxy_reports_worker_exit_during_initialization() {
    finishes_promptly(|| {
        let result = RustEngineProxy::spawn_with_initializer(|| panic!("startup failure"));
        assert!(matches!(result, Err(EngineError::Spawn(_))));
    });
}

#[test]
fn parse_fullform_basic() {
    let e = Expr::parse_fullform("(+ (+ (^ x 2) (* 2 x)) 1)").unwrap();
    assert_eq!(
        e,
        Expr::Call {
            head: "+".into(),
            args: vec![
                Expr::Call {
                    head: "+".into(),
                    args: vec![
                        Expr::Call {
                            head: "^".into(),
                            args: vec![Expr::Symbol("x".into()), Expr::Number("2".into())]
                        },
                        Expr::Call {
                            head: "*".into(),
                            args: vec![Expr::Number("2".into()), Expr::Symbol("x".into())]
                        }
                    ]
                },
                Expr::Number("1".into())
            ]
        }
    );
    // 序列化回 Yacas 语法(嵌套二元运算每层都带括号,安全冗余)
    assert_eq!(e.to_string(), "(((x ^ 2) + (2 * x)) + 1)");
}

#[test]
fn parse_fullform_leaf() {
    assert_eq!(
        Expr::parse_fullform("2.5").unwrap(),
        Expr::Number("2.5".into())
    );
    assert_eq!(Expr::parse_fullform("x").unwrap(), Expr::Symbol("x".into()));
}

#[test]
fn parse_fullform_scientific_number() {
    assert_eq!(
        Expr::parse_fullform("0.6245947718e-1").unwrap(),
        Expr::Number("0.6245947718e-1".into())
    );
}

#[test]
fn display_unary_minus_roundtrip() {
    // 一元负号的 FullForm 形式为原子 `-x` 或 `(- x)`,Display 需可回读
    let e = Expr::parse_fullform("(- x)").unwrap();
    assert_eq!(e.to_string(), "(- x)");
    let e2 = Expr::parse_fullform("-x").unwrap();
    assert_eq!(e2.to_string(), "-x");
}

#[test]
fn engine_eval_returns_structured() {
    if !crate::engine::cpp_reference_available() {
        eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
        return;
    }
    let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
    let r = engine.eval("D(x) Sin(x)^2").expect("求值失败");
    // (* 2 (* (Cos x) (Sin x)))
    assert_eq!(
        r.expr,
        Expr::Call {
            head: "*".into(),
            args: vec![
                Expr::Number("2".into()),
                Expr::Call {
                    head: "*".into(),
                    args: vec![
                        Expr::Call {
                            head: "Cos".into(),
                            args: vec![Expr::Symbol("x".into())]
                        },
                        Expr::Call {
                            head: "Sin".into(),
                            args: vec![Expr::Symbol("x".into())]
                        }
                    ]
                }
            ]
        }
    );
    assert!(r.tex.contains("\\cos"), "TeX 异常: {}", r.tex);

    // 会话状态跨命令保持
    let _ = engine.eval("a := 5");
    let r = engine.eval("a^2").expect("状态保持失败");
    assert_eq!(r.expr, Expr::Number("25".into()));

    // 矩阵:多行 FullForm + 单行结果,验证后缀切分
    let r = engine.eval("{{1,2},{3,4}}").expect("矩阵求值失败");
    assert_eq!(
        r.expr,
        Expr::Call {
            head: "List".into(),
            args: vec![
                Expr::Call {
                    head: "List".into(),
                    args: vec![Expr::Number("1".into()), Expr::Number("2".into())]
                },
                Expr::Call {
                    head: "List".into(),
                    args: vec![Expr::Number("3".into()), Expr::Number("4".into())]
                }
            ]
        }
    );
}

#[test]
fn stepsd_works_through_engine() {
    if !crate::engine::cpp_reference_available() {
        eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
        return;
    }
    let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
    // 步骤层 v1:StepsD 返回 {规则名, 表达式} 列表
    let r = engine.eval("StepsD(x*Sin(x), x)").expect("StepsD 求值失败");
    assert!(
        matches!(r.expr, Expr::Call { ref head, .. } if head == "List"),
        "StepsD 应返回列表,实际: {}",
        r.expr
    );
    // 最终一步(化简后)与引擎 D 结果代数等价(用 Simplify 判定)
    let diff = engine
        .eval("Simplify(StepsD(x*Sin(x), x)[Length(StepsD(x*Sin(x), x))][2] - D(x)x*Sin(x))")
        .expect("最终步求值失败");
    assert_eq!(
        diff.expr.to_string(),
        "0",
        "StepsD 最终步与 D 不一致: {}",
        diff.expr
    );
}

#[test]
fn engine_reports_errors() {
    if !crate::engine::cpp_reference_available() {
        eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
        return;
    }
    let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
    // D 是 bodied 运算符,逗号形式是非法语法——应报错而非静默
    let err = engine.eval("D(x^2,x)").unwrap_err();
    assert!(err.to_string().contains("错误"), "应报告错误: {err}");
}

#[test]
fn d_bodied_syntax_works() {
    if !crate::engine::cpp_reference_available() {
        eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
        return;
    }
    let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
    // D 的合法语法(stdopers.ys:39):D(var)expr 与 D(var,order)expr
    let r = engine.eval("D(x) x^2").expect("D(x)x^2 失败");
    assert_eq!(r.expr.to_string(), "(2 * x)");
    let r = engine.eval("D(x,2) x^4").expect("D(x,2)x^4 失败");
    assert_eq!(r.expr.to_string(), "(12 * (x ^ 2))");
    let r = engine.eval("D(x) Sin(x)").expect("D(x)Sin(x) 失败");
    assert_eq!(r.expr.to_string(), "Cos(x)");
}

#[test]
fn rust_engine_proxy_end_to_end() {
    let mut e = RustEngineProxy::spawn().expect("proxy spawn");
    let r = e.eval("D(x) Sin(x)").expect("eval");
    assert_eq!(r.expr.to_string(), "Cos(x)");
    assert!(r.tex.contains("\\cos"), "TeX 异常: {}", r.tex);
    let r = e.eval("Integrate(x) x^2").expect("eval2");
    let s = r.expr.to_string();
    assert!(s.contains("x") && s.contains("3"), "积分异常: {s}");
}

#[test]
fn engine_recovers_after_timeout() {
    if !crate::engine::cpp_reference_available() {
        eprintln!("skip: C++ reference binary not available (set YACAS_BIN to enable)");
        return;
    }
    let mut engine = ReplEngine::spawn().expect("启动 yacas 失败");
    // 死循环触发超时(引擎被终止)
    let err = engine.eval("While(True) 1").unwrap_err();
    assert!(matches!(err, EngineError::Timeout(_)), "应报超时: {err}");
    // 下一次调用自动重启并恢复工作
    let r = engine.eval("D(x) Sin(x)^2").expect("重启后应恢复");
    assert!(r.tex.contains("\\cos"), "TeX 异常: {}", r.tex);
}

#[test]
fn errors_have_stable_codes_and_retry_policy() {
    for (error, code, retryable) in [
        (
            EngineError::InvalidInput("bad input".into()),
            ErrorCode::InvalidInput,
            false,
        ),
        (
            EngineError::Eval("failed".into()),
            ErrorCode::EvaluationFailed,
            false,
        ),
        (
            EngineError::Timeout("slow".into()),
            ErrorCode::Timeout,
            true,
        ),
        (
            EngineError::Spawn("offline".into()),
            ErrorCode::EngineUnavailable,
            true,
        ),
        (
            EngineError::Io("closed".into()),
            ErrorCode::EngineUnavailable,
            true,
        ),
        (
            EngineError::Parse("bad output".into()),
            ErrorCode::Internal,
            false,
        ),
    ] {
        let response = error.response();
        assert_eq!(response.code, code);
        assert_eq!(response.retryable, retryable);
        assert!(!response.message.is_empty());
    }
}

#[test]
fn runtime_local_symbols_do_not_accumulate_between_requests() {
    let mut engine = RustEngine::spawn().unwrap();
    let expression = "Simplify(2*x*y+3+(x^2+4*y)*y')";

    // The first request may retain bindings required by a newly lazy-loaded
    // script. Once loaded, runtime hygienic symbols must remain request-local.
    engine.eval_expr(expression).unwrap();
    engine.eval_expr(expression).unwrap();
    let globals = engine.env.globals.len();
    let symbols = engine.env.symtab.len();
    for _ in 0..3 {
        engine.eval_expr(expression).unwrap();
        assert_eq!(engine.env.globals.len(), globals);
        assert_eq!(engine.env.symtab.len(), symbols);
    }
}
