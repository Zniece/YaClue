use std::fmt;

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
        let head = tokens.get(*pos).ok_or("FullForm 解析:缺少 head")?.clone();
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
        !t.is_empty() && t.chars().any(|c| c.is_ascii_digit()) && t.parse::<f64>().is_ok()
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

    /// 只返回结构化结果。无需展示整个结果时，实现可跳过 TeXForm。
    fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
        self.eval(command).map(|result| result.expr)
    }

    /// 在同一宿主请求中依次求值表达式并生成 TeX。实现可覆盖此方法，
    /// 以复用解析器、Environment、截止时间和线程往返。
    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        expressions
            .iter()
            .map(|expression| self.eval(expression).map(|result| result.tex))
            .collect()
    }

    /// 追踪信息:命中的规则列表(供步骤层使用);未实现时返回空
    #[allow(dead_code)] // 步骤层接口,暂未消费
    fn trace(&mut self, _command: &str) -> Result<Vec<String>, EngineError> {
        Ok(vec![])
    }
}
