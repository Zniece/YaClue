use std::time::Duration;

use super::repl::{default_scripts_dir, default_steps_dir, steps_boot_cmds_from_dir};
use super::{Engine, EngineError, EvalResult, Expr};

pub struct RustEngine {
    pub env: yacas_rs::env::Environment,
}

impl RustEngine {
    /// 装载脚本库(scripts 目录默认 yacas/scripts,可用 YACAS_SCRIPTS 覆盖)
    pub fn spawn() -> Result<Self, EngineError> {
        let scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| default_scripts_dir());
        let steps = std::env::var("YACAS_STEPS_SCRIPTS").unwrap_or_else(|_| default_steps_dir());
        Self::spawn_with_scripts(scripts, steps)
    }

    pub(super) fn spawn_with_scripts(
        mut scripts: String,
        steps: String,
    ) -> Result<Self, EngineError> {
        // 与 cyacas main 注入 rootdir 的方式一致:目录以 '/' 结尾
        if !scripts.ends_with('/') {
            scripts.push('/');
        }
        let mut env = yacas_rs::env::Environment::new();
        eval_cmd(&mut env, &format!("DefaultDirectory(\"{scripts}\")"))
            .map_err(|e| EngineError::Spawn(format!("DefaultDirectory 失败: {e:?}")))?;
        eval_cmd(&mut env, "Load(\"yacasinit.ys\")")
            .map_err(|e| EngineError::Spawn(format!("装载 yacasinit.ys 失败: {e:?}")))?;
        for cmd in steps_boot_cmds_from_dir(&steps) {
            eval_cmd(&mut env, &cmd)
                .map_err(|e| EngineError::Spawn(format!("装载步骤包失败({cmd}): {e:?}")))?;
        }
        // Loading needs file access, but product evaluation must not expose
        // host shell or file commands unless a trusted caller explicitly
        // opts out through the public environment.
        env.secure = true;
        Ok(RustEngine { env })
    }
}

/// 解析 + 求值一条命令,返回求值结果对象
pub(super) fn eval_cmd(
    env: &mut yacas_rs::env::Environment,
    command: &str,
) -> Result<std::rc::Rc<yacas_rs::value::LispObject>, yacas_rs::errors::YacasError> {
    let tree = yacas_rs::parser::parse_expression(env, &format!("{command};"))
        .map_err(|e| yacas_rs::errors::YacasError::generic(format!("解析失败: {e:?}")))?
        .ok_or_else(|| yacas_rs::errors::YacasError::Generic("空表达式".into()))?;
    yacas_rs::evaluator::eval(env, &tree)
}

/// Rust 引擎单次求值超时(病态输入的兜底;正常用例远低于此值,θ 链最重 ~3s)
const RUST_EVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

impl RustEngine {
    /// One deadline covers command evaluation and TeX generation. All Result
    /// paths clear it so a failed request does not poison the next request.
    pub(super) fn eval_with_timeout(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<EvalResult, EngineError> {
        let first_unique_id = self.env.last_unique_id;
        let loaded_files = loaded_def_file_count(&self.env);
        self.env.set_eval_timeout(Some(timeout));
        let response = (|| {
            let result =
                eval_cmd(&mut self.env, command).map_err(|e| self.eval_error(e, "求值失败"))?;
            let fullform = yacas_rs::printer::full_form(&result);
            let expr = Expr::parse_fullform(&fullform).map_err(EngineError::Parse)?;

            // Pass the result object as Hold(result), without printing/parsing
            // it or executing the original command again. Hold also preserves
            // deliberately unevaluated expressions supplied by the caller.
            let tex_value = tex_form_value(&mut self.env, &result)
                .map_err(|e| self.eval_error(e, "TeXForm 失败"))?;
            let printed = yacas_rs::printer::infix_print(&self.env, &tex_value);
            let tex = unquote_printed(&printed);
            Ok(EvalResult { expr, tex })
        })();
        // The evaluator samples its clock; also check short requests and time
        // spent converting the result after the last evaluator clock sample.
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if loaded_def_file_count(&self.env) == loaded_files {
            self.env.clear_unique_globals_since(first_unique_id);
        }
        if response.is_ok() && expired {
            Err(EngineError::Timeout("求值或 TeX 生成超过时限".into()))
        } else {
            response
        }
    }

    fn eval_expr_with_timeout(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<Expr, EngineError> {
        let first_unique_id = self.env.last_unique_id;
        let loaded_files = loaded_def_file_count(&self.env);
        self.env.set_eval_timeout(Some(timeout));
        let response = (|| {
            let result = eval_cmd(&mut self.env, command)
                .map_err(|error| self.eval_error(error, "求值失败"))?;
            Expr::parse_fullform(&yacas_rs::printer::full_form(&result)).map_err(EngineError::Parse)
        })();
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if loaded_def_file_count(&self.env) == loaded_files {
            self.env.clear_unique_globals_since(first_unique_id);
        }
        if response.is_ok() && expired {
            Err(EngineError::Timeout("求值或结构化输出超过时限".into()))
        } else {
            response
        }
    }

    fn render_tex_batch_with_timeout(
        &mut self,
        expressions: &[String],
        timeout: Duration,
    ) -> Result<Vec<String>, EngineError> {
        let first_unique_id = self.env.last_unique_id;
        let loaded_files = loaded_def_file_count(&self.env);
        self.env.set_eval_timeout(Some(timeout));
        let response: Result<Vec<String>, EngineError> = expressions
            .iter()
            .map(|expression| {
                let result = eval_cmd(&mut self.env, expression)
                    .map_err(|error| self.eval_error(error, "批量求值失败"))?;
                let tex_value = tex_form_value(&mut self.env, &result)
                    .map_err(|error| self.eval_error(error, "批量 TeXForm 失败"))?;
                Ok(unquote_printed(&yacas_rs::printer::infix_print(
                    &self.env, &tex_value,
                )))
            })
            .collect();
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if loaded_def_file_count(&self.env) == loaded_files {
            self.env.clear_unique_globals_since(first_unique_id);
        }
        if response.is_ok() && expired {
            Err(EngineError::Timeout("批量求值或 TeX 生成超过时限".into()))
        } else {
            response
        }
    }

    fn deadline_expired(&self) -> bool {
        self.env
            .eval_deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
    }

    fn eval_error(&self, error: yacas_rs::errors::YacasError, stage: &str) -> EngineError {
        if matches!(error, yacas_rs::errors::YacasError::UserInterrupt) && self.deadline_expired() {
            EngineError::Timeout(format!("{stage}: 超过求值时限"))
        } else {
            EngineError::Eval(format!("{stage}: {error:?}"))
        }
    }
}

fn loaded_def_file_count(env: &yacas_rs::env::Environment) -> usize {
    env.def_files
        .map
        .values()
        .filter(|file| file.is_loaded)
        .count()
}

fn unquote_printed(printed: &str) -> String {
    let printed = printed.trim();
    printed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(printed)
        .to_string()
}

fn tex_form_value(
    env: &mut yacas_rs::env::Environment,
    value: &std::rc::Rc<yacas_rs::value::LispObject>,
) -> Result<std::rc::Rc<yacas_rs::value::LispObject>, yacas_rs::errors::YacasError> {
    use yacas_rs::value::{build_list, clone_kind, LispObject, ObjectKind};

    let held = build_list(vec![
        ObjectKind::Atom(env.symtab.look_up("Hold")),
        clone_kind(&value.kind),
    ])
    .expect("Hold has a head and an argument");
    let call = build_list(vec![
        ObjectKind::Atom(env.symtab.look_up("TeXForm")),
        ObjectKind::Sublist(held),
    ])
    .expect("TeXForm has a head and an argument");
    yacas_rs::evaluator::eval(env, &LispObject::new(ObjectKind::Sublist(call)))
}

impl Engine for RustEngine {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        self.eval_with_timeout(command, RUST_EVAL_TIMEOUT)
    }

    fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
        self.eval_expr_with_timeout(command, RUST_EVAL_TIMEOUT)
    }

    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        self.render_tex_batch_with_timeout(expressions, RUST_EVAL_TIMEOUT)
    }
}
