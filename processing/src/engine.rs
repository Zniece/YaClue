// 引擎适配接口(加工层/步骤层只依赖本模块)
//
// 设计约定:
//   - 输入永远是 Yacas 语法字符串(步骤层本身就是生成 Yacas 命令的层);
//   - 输出是结构化 Expr(由 FullForm 解析)+ TeXForm,供步骤层导航与渲染;
//   - 引擎实现可互换:C++ 原版(子进程 REPL)与 Rust 移植版(进程内)都实现
//     同一个 `Engine` trait,上层零改动。
//
// 协议细节(已在 RESEARCH.md §6 实测):
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
            && t.chars().all(|c| c.is_ascii_digit() || c == '.')
            && t.chars().any(|c| c.is_ascii_digit())
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

#[derive(Debug)]
pub enum EngineError {
    Spawn(String),
    Io(String),
    /// yacas 报告的命令错误(原始输出)
    Eval(String),
    /// FullForm/TeXForm 输出解析失败
    Parse(String),
    /// 命令执行超时(疑似死循环),引擎已终止
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
}

impl RustEngine {
    /// 装载脚本库(scripts 目录默认 yacas/scripts,可用 YACAS_SCRIPTS 覆盖)
    pub fn spawn() -> Result<Self, EngineError> {
        let mut scripts = std::env::var("YACAS_SCRIPTS").unwrap_or_else(|_| default_scripts_dir());
        // 与 cyacas main 注入 rootdir 的方式一致:目录以 '/' 结尾
        if !scripts.ends_with('/') {
            scripts.push('/');
        }
        let mut env = yacas_rs::env::Environment::new();
        eval_cmd(&mut env, &format!("DefaultDirectory(\"{scripts}\")"))
            .map_err(|e| EngineError::Spawn(format!("DefaultDirectory 失败: {e:?}")))?;
        eval_cmd(&mut env, "Load(\"yacasinit.ys\")")
            .map_err(|e| EngineError::Spawn(format!("装载 yacasinit.ys 失败: {e:?}")))?;
        for cmd in steps_boot_cmds() {
            eval_cmd(&mut env, &cmd)
                .map_err(|e| EngineError::Spawn(format!("装载步骤包失败({cmd}): {e:?}")))?;
        }
        Ok(RustEngine { env })
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

impl Engine for RustEngine {
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError> {
        // 每次求值挂截止时间:引擎在 eval 循环内按 1024 操作采样检查,
        // 超时返回 UserInterrupt(病态输入不再挂死前端)
        self.env.set_eval_timeout(Some(RUST_EVAL_TIMEOUT));
        // 求值一次,同一结果对象打两种形式(比 ReplEngine 三次求值更忠实)
        let result = eval_cmd(&mut self.env, command).map_err(|e| EngineError::Eval(format!("{e:?}")))?;
        self.env.set_eval_timeout(None);
        let fullform = yacas_rs::printer::full_form(&result);
        let expr = Expr::parse_fullform(&fullform).map_err(EngineError::Parse)?;
        // TeXForm 需再求值一次(它是脚本层命令,作用于表达式本身)
        let tex = match eval_cmd(&mut self.env, &format!("TeXForm({command})")) {
            Ok(t) => {
                let s = yacas_rs::printer::infix_print(&self.env, &t);
                s.trim()
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .unwrap_or(s.trim())
                    .to_string()
            }
            // TeXForm 失败不拖垮结果本身(与 ReplEngine 行为对齐:那里失败会报错,
            // 但步骤层所有表达式都应可 TeX;此处保留严格性)
            Err(e) => return Err(EngineError::Eval(format!("TeXForm 失败: {e:?}"))),
        };
        Ok(EvalResult { expr, tex })
    }
}

/// RustEngine 的线程代理:`Environment` 内含 Rc/RefCell(非 Send),不能直接放进
/// Tauri 的 State<Mutex<>>。专职引擎线程独占 Environment,对外只收发 String/
/// EvalResult(均 Send),互斥由通道天然串行化 —— 架构与 ReplEngine 的子进程
/// +通道模式对齐,GUI 侧零感知。
pub struct RustEngineProxy {
    tx: Option<mpsc::Sender<String>>,
    rx: mpsc::Receiver<Result<EvalResult, String>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RustEngineProxy {
    pub fn spawn() -> Result<Self, EngineError> {
        let (tx, cmd_rx) = mpsc::channel::<String>();
        let (res_tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || match RustEngine::spawn() {
            Err(e) => {
                let _ = res_tx.send(Err(format!("{e:?}")));
                for _ in cmd_rx {} // 吸干命令,避免调用端 recv 挂死
            }
            Ok(mut eng) => {
                for cmd in cmd_rx {
                    let r = eng.eval(&cmd).map_err(|e| format!("{e:?}"));
                    if res_tx.send(r).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(RustEngineProxy { tx: Some(tx), rx, handle: Some(handle) })
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
        tx.send(command.to_string())
            .map_err(|e| EngineError::Io(e.to_string()))?;
        self.rx
            .recv()
            .map_err(|e| EngineError::Io(e.to_string()))?
            .map_err(EngineError::Eval)
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        // D 是 bodied 运算符,逗号形式是非法语法(见 RESEARCH §6.3)——应报错而非静默
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
