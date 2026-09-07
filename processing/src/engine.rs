// 引擎适配接口(加工层/步骤层只依赖本模块)
//
// 设计约定:
//   - 输入永远是 Yacas 语法字符串(步骤层本身就是生成 Yacas 命令的层);
//   - 输出是结构化 Expr(由 FullForm 解析)+ TeXForm,供步骤层导航与渲染;
//   - 引擎实现可互换:C++ 原版(子进程 REPL)与 Rust 移植版(进程内)都实现
//     同一个 `Engine` trait,上层零改动。
//
// C++ 子进程适配器的协议细节:
//   - `yacas -pc --rootdir <scripts>`:无提示符,每条输入后 flush;
//   - 哨兵 `"__YACAS_END__"` 分隔每条命令的输出;
//   - `FullForm(expr)` 输出前缀嵌套形式,如 `(+ (+ (^ x 2) (* 2 x)) 1)`;
//   - `TeXForm(expr)` 输出 `"$...$"` 带引号字符串。

use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const SENTINEL: &str = "\"__YACAS_END__\"";
const BANNER_END: &str = "keep typing Example();";
/// 单条命令的执行超时(防死循环挂死 GUI);超时后终止引擎并可自动重启
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);

// ============================================================
// 结构化表达式(引擎无关)
// ============================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// 数字(保持原文,如 "2", "2.5", "1/3" 的分子分母会被拆成 Call)
    Number(String),
    /// 符号/变量/函数名/运算符名(如 "x", "Sin", "+")
    Symbol(String),
    /// 复合表达式:head + 参数列表(如 (+ x 1)、(Sin x))
    Call { head: String, args: Vec<Expr> },
}

impl fmt::Display for Expr {
    /// 序列化为 Yacas 语法(加括号避免优先级问题)
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(n) | Expr::Symbol(n) => write!(f, "{n}"),
            Expr::Call { head, args } => match head.as_str() {
                op @ ("+" | "*") if args.len() > 1 => {
                    let mut it = args.iter();
                    let first = it.next().expect("args 非空");
                    write!(f, "({first}")?;
                    for a in it {
                        write!(f, " {op} {a}")?;
                    }
                    write!(f, ")")
                }
                op @ ("-" | "/" | "^" | "=" | "!=" | "<" | ">" | "<=" | ">=") => {
                    if args.len() == 1 {
                        // 一元运算符(如 -a)
                        write!(f, "({op} {})", args[0])
                    } else {
                        write!(f, "({} {} {})", args[0], op, args[1])
                    }
                }
                _ => {
                    write!(f, "{head}(")?;
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ",")?;
                        }
                        write!(f, "{a}")?;
                    }
                    write!(f, ")")
                }
            },
        }
    }
}

impl Expr {
    /// 解析 Yacas FullForm 输出(如 `(+ (+ (^ x 2) (* 2 x)) 1)`)
    pub fn parse_fullform(s: &str) -> Result<Expr, String> {
        let tokens = tokenize_fullform(s);
        let mut pos = 0;
        let expr = parse_node(&tokens, &mut pos)?;
        if pos != tokens.len() {
            return Err(format!("FullForm 解析:存在未消费的 token(位置 {pos})"));
        }
        Ok(expr)
    }
}

/// 分词:空白分隔,且 `(`/`)` 独立成 token(真实输出中会粘连,如 `))1`)。
/// 字符串字面量(`"..."`,含转义)整体作为一个 token —— 文案等含空格的字符串
/// 不会被拆开。
fn tokenize_fullform(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut atom = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' | ')' => {
                if !atom.is_empty() {
                    tokens.push(std::mem::take(&mut atom));
                }
                tokens.push(c.to_string());
            }
            c if c.is_whitespace() => {
                if !atom.is_empty() {
                    tokens.push(std::mem::take(&mut atom));
                }
            }
            '"' => {
                // 字符串字面量:吞到闭合引号(`\` 转义下一个字符),整体入 token
                if !atom.is_empty() {
                    tokens.push(std::mem::take(&mut atom));
                }
                let mut lit = String::new();
                lit.push('"');
                while let Some(&c2) = chars.peek() {
                    chars.next();
                    lit.push(c2);
                    if c2 == '\\' {
                        if let Some(&c3) = chars.peek() {
                            chars.next();
                            lit.push(c3);
                        }
                    } else if c2 == '"' {
                        break;
                    }
                }
                tokens.push(lit);
            }
            c => atom.push(c),
        }
    }
    if !atom.is_empty() {
        tokens.push(atom);
    }
    tokens
}

fn parse_node(tokens: &[String], pos: &mut usize) -> Result<Expr, String> {
    let tok = tokens.get(*pos).ok_or("FullForm 解析:意外结束")?.clone();
    *pos += 1;
    if tok == "(" {
        let head = tokens
            .get(*pos)
            .ok_or("FullForm 解析:缺少 head")?
            .clone();
        if head == "(" || head == ")" {
            return Err(format!("FullForm 解析:非法 head '{head}'"));
        }
        *pos += 1;
        let mut args = Vec::new();
        loop {
            let next = tokens.get(*pos).ok_or("FullForm 解析:括号未闭合")?;
            if next == ")" {
                *pos += 1;
                break;
            }
            args.push(parse_node(tokens, pos)?);
        }
        Ok(Expr::Call { head, args })
    } else if tok == ")" {
        Err("FullForm 解析:多余的 ')'".into())
    } else {
        Ok(classify_atom(&tok))
    }
}

fn classify_atom(tok: &str) -> Expr {
    let is_number = {
        let t = tok.strip_prefix(['+', '-']).unwrap_or(tok);
        !t.is_empty()
            && t.chars().any(|c| c.is_ascii_digit())
            && t.parse::<f64>().is_ok()
    };
    if is_number {
        Expr::Number(tok.to_string())
    } else {
        Expr::Symbol(tok.to_string())
    }
}

// ============================================================
// 引擎接口
// ============================================================

/// 一次求值的结果
#[derive(Debug, Clone)]
pub struct EvalResult {
    /// 结构化结果(由 FullForm 解析)
    pub expr: Expr,
    /// TeXForm(已去引号,可直接喂 KaTeX)
    pub tex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExpressionHandle {
    session: u64,
    id: u64,
}

#[derive(Debug, Clone)]
pub struct RetainedEvalResult {
    pub result: EvalResult,
    pub handle: Option<ExpressionHandle>,
}

#[derive(Debug)]
pub enum EngineError {
    Spawn(String),
    Io(String),
    /// yacas 报告的命令错误(原始输出)
    Eval(String),
    /// FullForm/TeXForm 输出解析失败
    Parse(String),
    /// 命令执行超时；Rust 引擎中断当前求值，C++ 代理终止子进程
    Timeout(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Spawn(m) => write!(f, "引擎启动失败: {m}"),
            EngineError::Io(m) => write!(f, "引擎 I/O 错误: {m}"),
            EngineError::Eval(m) => write!(f, "命令错误: {m}"),
            EngineError::Parse(m) => write!(f, "输出解析失败: {m}"),
            EngineError::Timeout(m) => write!(f, "执行超时: {m}"),
        }
    }
}

/// 引擎适配接口:C++ 原版与 Rust 移植版都实现它
pub trait Engine {
    /// 执行一条 Yacas 命令,返回结构化结果 + TeXForm
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError>;

    /// 求值并在支持它的引擎会话中暂存原生结果树。
    fn eval_retained(&mut self, command: &str) -> Result<RetainedEvalResult, EngineError> {
        Ok(RetainedEvalResult { result: self.eval(command)?, handle: None })
    }

    /// 在同一宿主请求中依次求值表达式并生成 TeX。实现可覆盖此方法，
    /// 以复用解析器、Environment、截止时间和线程往返。
    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        expressions
            .iter()
            .map(|expression| self.eval(expression).map(|result| result.tex))
            .collect()
    }

    /// 消费暂存结果句柄，沿零基参数路径取得子表达式并批量渲染。
    /// 不支持句柄的实现使用 `fallback` 表达式。
    fn render_tex_fields(
        &mut self,
        handle: Option<ExpressionHandle>,
        _paths: &[Vec<usize>],
        fallback: &[String],
    ) -> Result<Vec<String>, EngineError> {
        let _ = handle;
        self.render_tex_batch(fallback)
    }

    /// 追踪信息:命中的规则列表(供步骤层使用);未实现时返回空
    #[allow(dead_code)] // 步骤层接口,暂未消费
    fn trace(&mut self, _command: &str) -> Result<Vec<String>, EngineError> {
        Ok(vec![])
    }
}

/// yacas 命令出错时的输出特征(启发式,逐行匹配)
const ERROR_MARKERS: [&str; 9] = [
    "In function",
    "Invalid argument",
    "Wrong number of arguments",
    "bad argument number",
    "Expecting",
    "Could not create",
    "execution error",
    "Error parsing expression",
    "Argument is not a list",
];

fn looks_like_error(output: &str) -> bool {
    output
        .lines()
        .any(|line| ERROR_MARKERS.iter().any(|m| line.contains(m)))
}

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
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| EngineError::Spawn(format!("无法启动 yacas({bin}): {e}")))?;

        let stdin = child.stdin.take().ok_or(EngineError::Spawn("stdin 不可用".into()))?;
        let stdout = child.stdout.take().ok_or(EngineError::Spawn("stdout 不可用".into()))?;

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
        writeln!(self.stdin, "{command};")
            .and_then(|_| writeln!(self.stdin, "{SENTINEL};"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| EngineError::Io(format!("写入 yacas 失败: {e}")))?;

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
                    return Err(EngineError::Io("yacas 子进程意外退出".into()));
                }
            }
        }
    }
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
        let raw = self.eval_raw(command)?;
        if looks_like_error(&raw) {
            return Err(EngineError::Eval(raw));
        }
        // 结构化结果:FullForm(expr) 减去尾部重复的结果行
        let fullform_raw = self.eval_raw(&format!("FullForm({command})"))?;
        if looks_like_error(&fullform_raw) {
            return Err(EngineError::Eval(fullform_raw));
        }
        let fullform = strip_suffix_lines(&fullform_raw, &raw);
        let expr = Expr::parse_fullform(&fullform).map_err(EngineError::Parse)?;
        // TeXForm(expr)
        let tex_raw = self.eval_raw(&format!("TeXForm({command})"))?;
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

fn default_scripts_dir() -> String {
    // 引擎与加工层分离:scripts/ 位于加工层基座 yacas/ 下
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    root.join("yacas/scripts").to_string_lossy().into_owned()
}

/// 步骤层脚本目录(steps.rep 已从 yacas/scripts 剥离,归 processing 所有)
fn default_steps_dir() -> String {
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

fn steps_boot_cmds_from_dir(dir: &str) -> Vec<String> {
    vec![
        format!("DefaultDirectory(\"{dir}/\")"),
        "Load(\"steps.rep/code.ys\")".to_string(),
    ]
}

// ============================================================
// Rust 引擎(yacas-rs)进程内实现 —— 替换 ReplEngine 的目标形态
// ============================================================

/// 进程内 Rust 引擎:无子进程、无哨兵协议、无超时自杀。
/// 求值出错直接从 yacas-rs 拿到 YacasError,不再依赖错误标记启发式。
pub struct RustEngine {
    pub env: yacas_rs::env::Environment,
    session_id: u64,
    next_handle: u64,
    retained: std::collections::HashMap<u64, std::rc::Rc<yacas_rs::value::LispObject>>,
}

impl RustEngine {
    /// 装载脚本库(scripts 目录默认 yacas/scripts,可用 YACAS_SCRIPTS 覆盖)
    pub fn spawn() -> Result<Self, EngineError> {
        let scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| default_scripts_dir());
        let steps = std::env::var("YACAS_STEPS_SCRIPTS").unwrap_or_else(|_| default_steps_dir());
        Self::spawn_with_scripts(scripts, steps)
    }

    fn spawn_with_scripts(mut scripts: String, steps: String) -> Result<Self, EngineError> {
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
        static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Ok(RustEngine {
            env,
            session_id: NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            next_handle: 1,
            retained: std::collections::HashMap::new(),
        })
    }
}

/// 解析 + 求值一条命令,返回求值结果对象
fn eval_cmd(
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
    fn eval_with_timeout(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<EvalResult, EngineError> {
        self.env.set_eval_timeout(Some(timeout));
        let response = (|| {
            let result = eval_cmd(&mut self.env, command)
                .map_err(|e| self.eval_error(e, "求值失败"))?;
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
        if response.is_ok() && expired {
            Err(EngineError::Timeout("求值或 TeX 生成超过时限".into()))
        } else {
            response
        }
    }

    fn render_tex_batch_with_timeout(
        &mut self,
        expressions: &[String],
        timeout: Duration,
    ) -> Result<Vec<String>, EngineError> {
        self.env.set_eval_timeout(Some(timeout));
        let response: Result<Vec<String>, EngineError> = expressions
            .iter()
            .map(|expression| {
                let result = eval_cmd(&mut self.env, expression)
                    .map_err(|error| self.eval_error(error, "批量求值失败"))?;
                let tex_value = tex_form_value(&mut self.env, &result)
                    .map_err(|error| self.eval_error(error, "批量 TeXForm 失败"))?;
                Ok(unquote_printed(&yacas_rs::printer::infix_print(
                    &self.env,
                    &tex_value,
                )))
            })
            .collect();
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if response.is_ok() && expired {
            Err(EngineError::Timeout("批量求值或 TeX 生成超过时限".into()))
        } else {
            response
        }
    }

    fn eval_retained_with_timeout(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<RetainedEvalResult, EngineError> {
        self.env.set_eval_timeout(Some(timeout));
        let response = (|| {
            let value = eval_cmd(&mut self.env, command)
                .map_err(|error| self.eval_error(error, "求值失败"))?;
            let expr = Expr::parse_fullform(&yacas_rs::printer::full_form(&value))
                .map_err(EngineError::Parse)?;
            let tex_value = tex_form_value(&mut self.env, &value)
                .map_err(|error| self.eval_error(error, "TeXForm 失败"))?;
            let result = EvalResult {
                expr,
                tex: unquote_printed(&yacas_rs::printer::infix_print(&self.env, &tex_value)),
            };
            let id = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            self.retained.insert(id, value);
            Ok(RetainedEvalResult {
                result,
                handle: Some(ExpressionHandle { session: self.session_id, id }),
            })
        })();
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if response.is_ok() && expired {
            if let Ok(retained) = &response {
                if let Some(handle) = retained.handle {
                    self.retained.remove(&handle.id);
                }
            }
            Err(EngineError::Timeout("求值或 TeX 生成超过时限".into()))
        } else {
            response
        }
    }

    fn render_tex_fields_with_timeout(
        &mut self,
        handle: ExpressionHandle,
        paths: &[Vec<usize>],
        fallback: &[String],
        timeout: Duration,
    ) -> Result<Vec<String>, EngineError> {
        if handle.session != self.session_id {
            return Err(EngineError::Eval("表达式句柄不属于当前引擎会话".into()));
        }
        let root = self
            .retained
            .remove(&handle.id)
            .ok_or_else(|| EngineError::Eval("表达式句柄不存在或已被消费".into()))?;
        self.env.set_eval_timeout(Some(timeout));
        if paths.len() != fallback.len() {
            return Err(EngineError::Parse("字段路径与回退表达式数量不一致".into()));
        }
        let response: Result<Vec<String>, EngineError> = paths
            .iter()
            .zip(fallback)
            .map(|(path, fallback)| {
                let field = expression_at_path(&root, path)?;
                // Stored trees normally avoid all reparsing. Negative arguments of odd
                // functions are the one presentation-sensitive case where top-level
                // parsing performs a useful canonical rewrite (Sin(-u) -> -Sin(u)).
                let value = if contains_negative_odd_function(&field) {
                    eval_cmd(&mut self.env, fallback)
                        .map_err(|error| self.eval_error(error, "展示规范化失败"))?
                } else {
                    yacas_rs::evaluator::eval(&mut self.env, &field)
                        .map_err(|error| self.eval_error(error, "字段求值失败"))?
                };
                let tex_value = tex_form_value(&mut self.env, &value)
                    .map_err(|error| self.eval_error(error, "字段 TeXForm 失败"))?;
                Ok(unquote_printed(&yacas_rs::printer::infix_print(
                    &self.env,
                    &tex_value,
                )))
            })
            .collect();
        let expired = self.deadline_expired();
        self.env.set_eval_timeout(None);
        if response.is_ok() && expired {
            Err(EngineError::Timeout("字段批量求值或 TeX 生成超过时限".into()))
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
        if matches!(error, yacas_rs::errors::YacasError::UserInterrupt)
            && self.deadline_expired()
        {
            EngineError::Timeout(format!("{stage}: 超过求值时限"))
        } else {
            EngineError::Eval(format!("{stage}: {error:?}"))
        }
    }
}

fn unquote_printed(printed: &str) -> String {
    let printed = printed.trim();
    printed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(printed)
        .to_string()
}

fn expression_at_path(
    root: &std::rc::Rc<yacas_rs::value::LispObject>,
    path: &[usize],
) -> Result<std::rc::Rc<yacas_rs::value::LispObject>, EngineError> {
    use yacas_rs::value::{copy_node, spine_refs};

    let mut current = copy_node(root);
    for index in path {
        let list = current
            .sublist()
            .ok_or_else(|| EngineError::Parse("表达式路径经过了非复合节点".into()))?;
        let next = spine_refs(list)
            .nth(index + 1)
            .map(copy_node)
            .ok_or_else(|| EngineError::Parse("表达式路径索引越界".into()))?;
        current = next;
    }
    Ok(current)
}

fn contains_negative_odd_function(value: &std::rc::Rc<yacas_rs::value::LispObject>) -> bool {
    use yacas_rs::value::{spine_refs, ObjectKind};

    let ObjectKind::Sublist(list) = &value.kind else { return false };
    let nodes: Vec<_> = spine_refs(list).collect();
    let is_odd = nodes.first().and_then(|node| node.atom_string()).is_some_and(|head| {
        matches!(head.as_ref(), "Sin" | "Tan" | "Sinh" | "Tanh" | "ArcSin" | "ArcTan")
    });
    if is_odd && nodes.get(1).is_some_and(|argument| {
        let Some(product) = argument.sublist() else { return false };
        let mut parts = spine_refs(product);
        parts.next().and_then(|node| node.atom_string()).is_some_and(|head| head.as_ref() == "*")
            && parts.next().and_then(|node| node.number_string()).is_some_and(|n| n.starts_with('-'))
    }) {
        return true;
    }
    nodes.iter().skip(1).any(|node| contains_negative_odd_function(node))
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

    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        self.render_tex_batch_with_timeout(expressions, RUST_EVAL_TIMEOUT)
    }

    fn eval_retained(&mut self, command: &str) -> Result<RetainedEvalResult, EngineError> {
        self.eval_retained_with_timeout(command, RUST_EVAL_TIMEOUT)
    }

    fn render_tex_fields(
        &mut self,
        handle: Option<ExpressionHandle>,
        paths: &[Vec<usize>],
        fallback: &[String],
    ) -> Result<Vec<String>, EngineError> {
        match handle {
            Some(handle) => {
                self.render_tex_fields_with_timeout(handle, paths, fallback, RUST_EVAL_TIMEOUT)
            }
            None => self.render_tex_batch(fallback),
        }
    }
}

/// RustEngine 的线程代理:`Environment` 内含 Rc/RefCell(非 Send),不能直接放进
/// Tauri 的 State<Mutex<>>。专职引擎线程独占 Environment,对外只收发可 Send 的
/// 请求/响应枚举,互斥由通道天然串行化 —— 架构与 ReplEngine 的子进程
/// +通道模式对齐,GUI 侧零感知。
enum EngineRequest {
    Eval(String),
    EvalRetained(String),
    RenderTex(Vec<String>),
    RenderFields {
        handle: Option<ExpressionHandle>,
        paths: Vec<Vec<usize>>,
        fallback: Vec<String>,
    },
}

enum EngineResponse {
    Eval(Result<EvalResult, EngineError>),
    EvalRetained(Result<RetainedEvalResult, EngineError>),
    RenderTex(Result<Vec<String>, EngineError>),
    RenderFields(Result<Vec<String>, EngineError>),
}

pub struct RustEngineProxy {
    tx: Option<mpsc::Sender<EngineRequest>>,
    rx: mpsc::Receiver<EngineResponse>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RustEngineProxy {
    pub fn spawn() -> Result<Self, EngineError> {
        Self::spawn_with_initializer(RustEngine::spawn)
    }

    fn spawn_with_initializer(
        initialize: impl FnOnce() -> Result<RustEngine, EngineError> + Send + 'static,
    ) -> Result<Self, EngineError> {
        let (tx, cmd_rx) = mpsc::channel::<EngineRequest>();
        let (res_tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("yacas-engine".into())
            .spawn(move || {
                let mut engine = match initialize() {
                    Ok(engine) => engine,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }
                for request in cmd_rx {
                    let response = match request {
                        EngineRequest::Eval(command) => EngineResponse::Eval(engine.eval(&command)),
                        EngineRequest::EvalRetained(command) => {
                            EngineResponse::EvalRetained(engine.eval_retained(&command))
                        }
                        EngineRequest::RenderTex(expressions) => {
                            EngineResponse::RenderTex(engine.render_tex_batch(&expressions))
                        }
                        EngineRequest::RenderFields { handle, paths, fallback } => {
                            EngineResponse::RenderFields(
                                engine.render_tex_fields(handle, &paths, &fallback),
                            )
                        }
                    };
                    if res_tx.send(response).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| EngineError::Spawn(format!("无法启动引擎线程: {e}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(RustEngineProxy { tx: Some(tx), rx, handle: Some(handle) }),
            startup => {
                // Close commands before joining, including a worker that exits
                // without a startup response. No unusable proxy escapes.
                drop(tx);
                let _ = handle.join();
                Err(match startup {
                    Ok(Err(error)) => error,
                    Err(error) => EngineError::Spawn(format!("引擎线程初始化期间退出: {error}")),
                    Ok(Ok(())) => unreachable!(),
                })
            }
        }
    }
}

impl Drop for RustEngineProxy {
    fn drop(&mut self) {
        drop(self.tx.take()); // 关闭命令通道 → 引擎线程 for 循环结束 → join 可返回
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Engine for RustEngineProxy {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        let tx = self.tx.as_ref().ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::Eval(command.to_string()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::Eval(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }

    fn eval_retained(&mut self, command: &str) -> Result<RetainedEvalResult, EngineError> {
        let tx = self.tx.as_ref().ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::EvalRetained(command.to_string()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::EvalRetained(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }

    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        let tx = self.tx.as_ref().ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::RenderTex(expressions.to_vec()))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::RenderTex(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }

    fn render_tex_fields(
        &mut self,
        handle: Option<ExpressionHandle>,
        paths: &[Vec<usize>],
        fallback: &[String],
    ) -> Result<Vec<String>, EngineError> {
        let tx = self.tx.as_ref().ok_or_else(|| EngineError::Io("引擎线程已退出".into()))?;
        tx.send(EngineRequest::RenderFields {
            handle,
            paths: paths.to_vec(),
            fallback: fallback.to_vec(),
        })
        .map_err(|e| EngineError::Io(e.to_string()))?;
        match self.rx.recv().map_err(|e| EngineError::Io(e.to_string()))? {
            EngineResponse::RenderFields(result) => result,
            _ => Err(EngineError::Io("引擎响应类型不匹配".into())),
        }
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
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
    fn retained_expression_fields_are_session_bound_and_consumed() {
        let mut engine = RustEngine::spawn().unwrap();
        let retained = engine.eval_retained("{3*2*x,Sin(-4*x)}").unwrap();
        let handle = retained.handle.unwrap();
        let paths = vec![vec![0], vec![1]];
        let expected = engine
            .render_tex_batch(&["3*2*x".into(), "Sin(-4*x)".into()])
            .unwrap();
        assert_eq!(
            engine.render_tex_fields(Some(handle), &paths, &[]).unwrap(),
            expected
        );
        assert!(matches!(
            engine.render_tex_fields(Some(handle), &paths, &[]),
            Err(EngineError::Eval(_))
        ));

        let retained = engine.eval_retained("{x}").unwrap();
        let handle = retained.handle.unwrap();
        let mut other = RustEngine::spawn().unwrap();
        assert!(matches!(
            other.render_tex_fields(Some(handle), &[vec![0]], &[]),
            Err(EngineError::Eval(_))
        ));
        assert_eq!(
            engine.render_tex_fields(Some(handle), &[vec![0]], &[]).unwrap(),
            vec!["$x$"]
        );
    }

    #[test]
    fn rust_eval_recovers_after_parse_and_tex_errors() {
        let mut engine = RustEngine::spawn().unwrap();
        eval_cmd(&mut engine.env, "1 # TeXForm(ReviewBrokenTex) <-- Check(False, \"broken TeX\")")
            .unwrap();
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
            eval_cmd(&mut engine.env, "1 # TeXForm(ReviewSlowTex) <-- While(True) 1")
                .unwrap();
            for command in ["While(True) 1", "ReviewSlowTex"] {
                let error = engine
                    .eval_with_timeout(command, Duration::from_millis(20))
                    .unwrap_err();
                assert!(matches!(error, EngineError::Timeout(_)), "{error}");
                assert!(engine.env.eval_deadline.is_none());
                let result = engine.eval("2+3").expect("engine recovers after interruption");
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
            assert_eq!(
                engine
                    .render_tex_batch(&["2+3".into(), "Sin(x)".into()])
                    .unwrap(),
                vec!["$5$", "$\\sin x$"]
            );
            let retained = engine.eval_retained("{x^2,3*x}").unwrap();
            assert_eq!(
                engine
                    .render_tex_fields(
                        retained.handle,
                        &[vec![0], vec![1]],
                        &["x^2".into(), "3*x".into()],
                    )
                    .unwrap(),
                vec!["$x ^{2}$", "$3 x$"]
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
                                args: vec![
                                    Expr::Number("2".into()),
                                    Expr::Symbol("x".into())
                                ]
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
        assert_eq!(Expr::parse_fullform("2.5").unwrap(), Expr::Number("2.5".into()));
        assert_eq!(Expr::parse_fullform("x").unwrap(), Expr::Symbol("x".into()));
    }

    #[test]
    fn parse_fullform_scientific_number() {
        assert_eq!(Expr::parse_fullform("0.6245947718e-1").unwrap(), Expr::Number("0.6245947718e-1".into()));
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
        assert!(
            matches!(err, EngineError::Timeout(_)),
            "应报超时: {err}"
        );
        // 下一次调用自动重启并恢复工作
        let r = engine.eval("D(x) Sin(x)^2").expect("重启后应恢复");
        assert!(r.tex.contains("\\cos"), "TeX 异常: {}", r.tex);
    }
}
