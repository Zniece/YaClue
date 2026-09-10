use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::{Engine, EngineError, EvalResult, Expr};

const SENTINEL: &str = "\"__YACAS_END__\"";
const BANNER_END: &str = "keep typing Example();";
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);
static RESULT_SYMBOL_ID: AtomicU64 = AtomicU64::new(0);

// ============================================================
// 原版引擎:yacas REPL 子进程
// ============================================================

pub struct ReplEngine {
    child: Child,
    stdin: ChildStdin,
    /// stdout 由读取线程送入通道,支持超时接收
    rx: mpsc::Receiver<String>,
    /// 引擎已终止(超时/崩溃);下次调用时自动重启
    dead: bool,
    /// Process-unique scratch symbol holding the one evaluated result while
    /// FullForm and TeXForm render it without re-running caller input.
    result_symbol: String,
}

impl ReplEngine {
    pub fn spawn() -> Result<Self, EngineError> {
        let mut engine = Self::spawn_raw()?;
        engine.drain_banner()?;
        // 结果统一单行打印(矩阵等也压成一行),便于 FullForm 输出切分
        let _ = engine.eval_raw("DefaultPrinter(True)")?;
        for cmd in steps_boot_cmds() {
            engine.eval_raw(&cmd)?;
        }
        Ok(engine)
    }

    /// 创建子进程 + stdout 读取线程,不等待横幅
    fn spawn_raw() -> Result<Self, EngineError> {
        let bin = std::env::var("YACAS_BIN").unwrap_or_else(|_| default_yacas_bin());
        let scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| default_scripts_dir());

        let mut child = Command::new(&bin)
            .args(["-pc", "--rootdir", &scripts])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Error protocol is carried on stdout. Discard the otherwise
            // unread diagnostic stream so a noisy child cannot fill a pipe
            // and deadlock.
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| EngineError::Spawn(format!("无法启动 yacas({bin}): {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or(EngineError::Spawn("stdin 不可用".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(EngineError::Spawn("stdout 不可用".into()))?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        Ok(ReplEngine {
            child,
            stdin,
            rx,
            dead: false,
            result_symbol: format!(
                "YaClue'ReplResult{}'{}",
                std::process::id(),
                RESULT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed)
            ),
        })
    }

    /// 引擎已终止时重启(超时/崩溃后下次调用自动恢复)
    fn respawn(&mut self) -> Result<(), EngineError> {
        let fresh = Self::spawn()?;
        *self = fresh;
        Ok(())
    }

    /// 消费启动横幅(读到 "keep typing Example();" 即结束;
    /// -pc 模式无提示符、无尾随空行,不能再多读)
    fn drain_banner(&mut self) -> Result<(), EngineError> {
        loop {
            match self.rx.recv_timeout(EVAL_TIMEOUT) {
                Ok(line) => {
                    if line.contains(BANNER_END) {
                        return Ok(());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(EngineError::Spawn("yacas 启动超时".into()));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(EngineError::Spawn("yacas 启动后立即退出".into()));
                }
            }
        }
    }

    /// 执行一条 yacas 命令,返回其输出行(不含哨兵行)
    fn eval_raw(&mut self, command: &str) -> Result<String, EngineError> {
        if self.dead {
            self.respawn()?;
        }
        if let Err(error) = writeln!(self.stdin, "{command};")
            .and_then(|_| writeln!(self.stdin, "{SENTINEL};"))
            .and_then(|_| self.stdin.flush())
        {
            self.dead = true;
            return Err(EngineError::Io(format!("写入 yacas 失败: {error}")));
        }

        let mut lines = Vec::new();
        loop {
            match self.rx.recv_timeout(EVAL_TIMEOUT) {
                Ok(line) => {
                    if line == SENTINEL {
                        return Ok(lines.join("\n"));
                    }
                    lines.push(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // 疑似死循环:终止引擎,标记 dead,下次调用自动重启
                    self.dead = true;
                    let _ = self.child.kill();
                    return Err(EngineError::Timeout(
                        "命令执行超时(疑似死循环),引擎已终止".into(),
                    ));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.dead = true;
                    return Err(EngineError::Io("yacas 子进程意外退出".into()));
                }
            }
        }
    }

    fn eval_input_once(&mut self, command: &str) -> Result<String, EngineError> {
        let (protocol, marker) = trapped_eval_command(&self.result_symbol, command);
        let raw = self.eval_raw(&protocol)?;
        if let Some(message) = trapped_error(&raw, &marker) {
            Err(EngineError::Eval(message.to_string()))
        } else {
            Ok(raw)
        }
    }

    fn eval_trapped(&mut self, command: &str) -> Result<String, EngineError> {
        let marker = format!("__YACLUE_ERROR_{}__", self.result_symbol);
        let protocol = format!(
            "TrapError(({command}),ConcatStrings({},GetCoreError()))",
            yacas_string_literal(&marker)
        );
        let raw = self.eval_raw(&protocol)?;
        if let Some(message) = trapped_error(&raw, &marker) {
            Err(EngineError::Eval(message.to_string()))
        } else {
            Ok(raw)
        }
    }
}

fn trapped_eval_command(result_symbol: &str, command: &str) -> (String, String) {
    let marker = format!("__YACLUE_ERROR_{result_symbol}__");
    let source = yacas_string_literal(command);
    let marker_literal = yacas_string_literal(&marker);
    (
        format!(
            "TrapError({result_symbol}:=Eval(FromString({source})Read()),\
             ConcatStrings({marker_literal},GetCoreError()))"
        ),
        marker,
    )
}

impl Drop for ReplEngine {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "Exit();");
        let _ = self.stdin.flush();
        let _ = self.child.kill();
    }
}

/// `FullForm(cmd)` 的输出 = 内部形式 + 结果行(与 `cmd` 单独求值完全一致)。
/// 去掉尾部与 `result_raw` 相同的行,即得纯 FullForm 文本。
fn strip_suffix_lines(fullform_raw: &str, result_raw: &str) -> String {
    let ff: Vec<&str> = fullform_raw.lines().collect();
    let r: Vec<&str> = result_raw.lines().collect();
    if ff.len() >= r.len() && !r.is_empty() && ff[ff.len() - r.len()..] == r[..] {
        ff[..ff.len() - r.len()].join("\n")
    } else {
        fullform_raw.to_string()
    }
}

impl Engine for ReplEngine {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        let result_symbol = self.result_symbol.clone();
        let raw = self.eval_input_once(command)?;
        // 结构化结果:FullForm(expr) 减去尾部重复的结果行
        let fullform_raw = self.eval_trapped(&format!("FullForm({result_symbol})"))?;
        let fullform = strip_suffix_lines(&fullform_raw, &raw);
        let expr = Expr::parse_fullform(&fullform).map_err(EngineError::Parse)?;
        // TeXForm(expr)
        let tex_raw = self.eval_trapped(&format!("TeXForm({result_symbol})"))?;
        let tex = tex_raw
            .trim()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(tex_raw.trim())
            .to_string();
        Ok(EvalResult { expr, tex })
    }

    fn trace(&mut self, command: &str) -> Result<Vec<String>, EngineError> {
        // TODO(步骤层):TraceRule 的具体语义与输出格式待调研后实现
        let raw = self.eval_raw(&format!("TraceRule({command})"))?;
        Ok(raw
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !l.trim().is_empty())
            .collect())
    }
}

fn trapped_error<'a>(raw: &'a str, marker: &str) -> Option<&'a str> {
    raw.trim()
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.strip_prefix(marker))
}

fn default_yacas_bin() -> String {
    // Optional C++ reference binary used only by the dual-engine comparison
    // tests; override with YACAS_BIN. It is NOT part of this repository —
    // build it from the upstream sources (github.com/grzegorzmazur/yacas)
    // if you want those tests to run.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    root.join("build-ref/cyacas/yacas/yacas")
        .to_string_lossy()
        .into_owned()
}

/// Whether the optional C++ reference binary is available. Tests that need
/// it skip silently (with a note) when it is absent, so a clean clone runs
/// green.
pub fn cpp_reference_available() -> bool {
    if std::env::var_os("YACAS_BIN").is_some() {
        return true;
    }
    std::path::Path::new(&default_yacas_bin()).is_file() || {
        // The repo-root build layout may also place the binary under
        // <root>/build-ref; check both spellings.
        let alt = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("yacas/build-ref/cyacas/yacas/yacas"))
            .unwrap_or_default();
        std::path::Path::new(&alt).is_file()
    }
}

pub(super) fn default_scripts_dir() -> String {
    // 引擎与加工层分离:scripts/ 位于加工层基座 yacas/ 下
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    root.join("yacas/scripts").to_string_lossy().into_owned()
}

/// 步骤层脚本目录(steps.rep 已从 yacas/scripts 剥离,归 processing 所有)
pub(super) fn default_steps_dir() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .to_string_lossy()
        .into_owned()
}

/// 引擎启动后装载步骤包的两条命令(yacasinit 之后执行);
/// 步骤包不再经 packages.ys 懒加载链登记,由加工层显式加载
fn steps_boot_cmds() -> Vec<String> {
    let dir = std::env::var("YACAS_STEPS_SCRIPTS").unwrap_or_else(|_| default_steps_dir());
    steps_boot_cmds_from_dir(&dir)
}

pub(super) fn steps_boot_cmds_from_dir(dir: &str) -> Vec<String> {
    let directory = yacas_directory_literal(dir);
    vec![
        format!("DefaultDirectory({directory})"),
        "Load(\"steps.rep/code.ys\")".to_string(),
    ]
}

/// Yacas script lookup accepts forward slashes on every supported desktop
/// platform. Normalize Windows separators before constructing language input
/// so a filesystem path can never introduce a Yacas escape sequence.
pub(super) fn yacas_directory_literal(value: &str) -> String {
    let value = value.strip_prefix(r"\\?\").unwrap_or(value);
    let mut portable = value.replace('\\', "/");
    if !portable.ends_with('/') {
        portable.push('/');
    }
    yacas_string_literal(&portable)
}

pub(super) fn yacas_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\t' => literal.push_str("\\t"),
            '\n' => literal.push_str("\\n"),
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn child_disconnect_marks_engine_dead() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut engine = ReplEngine {
            child,
            stdin,
            rx,
            dead: false,
            result_symbol: "YaClue'TestResult".into(),
        };

        assert!(matches!(engine.eval_raw("1"), Err(EngineError::Io(_))));
        assert!(engine.dead);
    }

    #[test]
    fn startup_paths_are_escaped_as_yacas_strings() {
        assert_eq!(
            yacas_string_literal("a\\b\";SystemCall(\"bad\")\n"),
            "\"a\\\\b\\\";SystemCall(\\\"bad\\\")\\n\""
        );
        assert_eq!(
            steps_boot_cmds_from_dir("a\";SystemCall(\"bad\")"),
            [
                "DefaultDirectory(\"a\\\";SystemCall(\\\"bad\\\")/\")",
                "Load(\"steps.rep/code.ys\")"
            ]
        );
        assert_eq!(
            yacas_directory_literal(r"C:\Users\znie\Desktop\YaClue\yacas\scripts"),
            r#""C:/Users/znie/Desktop/YaClue/yacas/scripts/""#
        );
        assert_eq!(
            steps_boot_cmds_from_dir(r"\\?\C:\Program Files\YaClue\processing\scripts")[0],
            r#"DefaultDirectory("C:/Program Files/YaClue/processing/scripts/")"#
        );

        let mut env = yacas_rs::env::Environment::new();
        crate::engine::rust::eval_cmd(
            &mut env,
            r#"DefaultDirectory("C:/Users/znie/Desktop/YaClue/yacas/scripts/")"#,
        )
        .unwrap();
        assert_eq!(
            env.input_directories.last().map(String::as_str),
            Some("C:/Users/znie/Desktop/YaClue/yacas/scripts/")
        );
    }

    #[test]
    fn trapped_protocol_evaluates_once_and_classifies_syntax_errors() {
        use crate::engine::rust::eval_cmd;

        let mut engine = crate::engine::RustEngine::spawn().unwrap();
        eval_cmd(&mut engine.env, "replProtocolCounter:=0").unwrap();
        let (protocol, _marker) = trapped_eval_command(
            "YaClue'ReplProtocolTest",
            "replProtocolCounter:=replProtocolCounter+1",
        );
        let value = eval_cmd(&mut engine.env, &protocol).unwrap();
        assert_eq!(yacas_rs::printer::infix_print(&engine.env, &value), "1");
        let counter = eval_cmd(&mut engine.env, "replProtocolCounter").unwrap();
        assert_eq!(yacas_rs::printer::infix_print(&engine.env, &counter), "1");

        let (protocol, marker) = trapped_eval_command("YaClue'ReplProtocolTest", "Sin(");
        let error = eval_cmd(&mut engine.env, &protocol).unwrap();
        let printed = yacas_rs::printer::infix_print(&engine.env, &error);
        assert!(trapped_error(&printed, &marker).is_some(), "{printed}");
    }
}
