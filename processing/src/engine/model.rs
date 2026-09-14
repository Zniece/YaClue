use serde::Serialize;
use std::fmt;
use std::rc::Rc;
use yacas_rs::value::{atom_or_number, build_list, clone_kind, LispObject, ObjectKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A number in source form. Fractions such as `1/3` are represented as calls.
    Number(String),
    /// A symbol, variable, function name, or operator name such as `x`, `Sin`, or `+`.
    Symbol(String),
    /// A compound expression: a head and arguments, such as `(+ x 1)` or `(Sin x)`.
    Call { head: String, args: Vec<Expr> },
}

impl fmt::Display for Expr {
    /// Serialize as Yacas syntax, adding parentheses to avoid precedence ambiguity.
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
                        // Unary operator, for example `-a`.
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
    /// Parse Yacas FullForm output such as `(+ (+ (^ x 2) (* 2 x)) 1)`.
    pub fn parse_fullform(s: &str) -> Result<Expr, String> {
        let tokens = tokenize_fullform(s);
        let mut pos = 0;
        let expr = parse_node(&tokens, &mut pos)?;
        if pos != tokens.len() {
            return Err(format!("FullForm 解析:存在未消费的 token(位置 {pos})"));
        }
        Ok(expr)
    }

    /// Adapt the engine's structured FullForm DTO directly into the shared
    /// canonical AST. `Expr::Call` is already prefix/full-form data, so infix
    /// and bodied surface syntax require no reparsing or precedence recovery.
    pub(crate) fn to_canonical_ast(
        &self,
        env: &mut yacas_rs::env::Environment,
    ) -> Result<Rc<LispObject>, EngineError> {
        match self {
            Expr::Number(value) | Expr::Symbol(value) => Ok(atom_or_number(&mut env.symtab, value)),
            Expr::Call { head, args } => {
                let mut kinds = Vec::with_capacity(args.len() + 1);
                kinds.push(ObjectKind::Atom(env.symtab.look_up(head)));
                for argument in args {
                    let node = argument.to_canonical_ast(env)?;
                    kinds.push(clone_kind(&node.kind));
                }
                let list = build_list(kinds)
                    .ok_or_else(|| EngineError::Parse("结构化引擎调用缺少 head".into()))?;
                Ok(LispObject::new(ObjectKind::Sublist(list)))
            }
        }
    }
}

pub(crate) fn canonical_call_ast(
    env: &mut yacas_rs::env::Environment,
    head: &str,
    args: &[Rc<LispObject>],
) -> Result<Rc<LispObject>, EngineError> {
    let mut kinds = Vec::with_capacity(args.len() + 1);
    kinds.push(ObjectKind::Atom(env.symtab.look_up(head)));
    kinds.extend(args.iter().map(|argument| clone_kind(&argument.kind)));
    let list =
        build_list(kinds).ok_or_else(|| EngineError::Parse("结构化引擎调用缺少 head".into()))?;
    Ok(LispObject::new(ObjectKind::Sublist(list)))
}

/// Tokenize on whitespace and emit `(` and `)` separately. Real output can
/// place them next to other tokens, as in `))1`. A quoted string, including
/// escapes, stays in one token so embedded whitespace is preserved.
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
                // Consume through the closing quote; `\` escapes the next character.
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
// Engine interface
// ============================================================

/// The result of one evaluation.
#[derive(Debug, Clone)]
pub struct EvalResult {
    /// Structured result parsed from FullForm.
    pub expr: Expr,
    /// Unquoted TeXForm ready for KaTeX.
    pub tex: String,
}

#[derive(Debug)]
pub enum EngineError {
    /// Request syntax, arguments or product-level resource bounds are invalid.
    InvalidInput(String),
    Spawn(String),
    Io(String),
    /// Raw command error reported by Yacas.
    Eval(String),
    /// Failed to parse FullForm or TeXForm output.
    Parse(String),
    /// Command evaluation exceeded its deadline and was interrupted.
    Timeout(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    EvaluationFailed,
    Timeout,
    EngineUnavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorResponse {
    pub code: ErrorCode,
    #[serde(skip_serializing)]
    pub message: String,
    pub message_ref: crate::messages::MessageRef,
    pub retryable: bool,
}

impl ErrorResponse {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        let message = message.into();
        let key = match code {
            ErrorCode::InvalidInput => "errors.invalid_input",
            ErrorCode::EvaluationFailed => "errors.evaluation_failed",
            ErrorCode::Timeout => "errors.timeout",
            ErrorCode::EngineUnavailable => "errors.engine_unavailable",
            ErrorCode::Internal => "errors.internal",
        };
        Self {
            code,
            message_ref: crate::messages::MessageRef::new(key),
            message,
            retryable,
        }
    }

    pub fn keyed(
        code: ErrorCode,
        key: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            message_ref: crate::messages::MessageRef::new(key),
            retryable,
        }
    }

    pub fn arg(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.message_ref = self.message_ref.arg(name, value);
        self
    }
}

impl EngineError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidInput(_) => ErrorCode::InvalidInput,
            Self::Eval(_) => ErrorCode::EvaluationFailed,
            Self::Timeout(_) => ErrorCode::Timeout,
            Self::Spawn(_) | Self::Io(_) => ErrorCode::EngineUnavailable,
            Self::Parse(_) => ErrorCode::Internal,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Timeout(_) | Self::Spawn(_) | Self::Io(_))
    }

    pub fn response(&self) -> ErrorResponse {
        ErrorResponse::new(self.code(), self.to_string(), self.retryable())
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::InvalidInput(m) => write!(f, "invalid input: {m}"),
            EngineError::Spawn(m) => write!(f, "engine startup failed: {m}"),
            EngineError::Io(m) => write!(f, "engine I/O failed: {m}"),
            EngineError::Eval(m) => write!(f, "evaluation failed: {m}"),
            EngineError::Parse(m) => write!(f, "engine output parsing failed: {m}"),
            EngineError::Timeout(m) => write!(f, "evaluation timed out: {m}"),
        }
    }
}

/// Evaluation boundary implemented by the Rust engine and usable by test doubles.
pub trait Engine {
    /// Execute one Yacas command and return its structured result and TeXForm.
    fn eval(&mut self, command: &str) -> Result<EvalResult, EngineError>;

    /// Return only the structured result. Implementations may skip TeXForm.
    fn eval_expr(&mut self, command: &str) -> Result<Expr, EngineError> {
        self.eval(command).map(|result| result.expr)
    }

    /// Render expressions sequentially in one host request. Implementations may
    /// override this to reuse a parser, environment, deadline, and thread hop.
    fn render_tex_batch(&mut self, expressions: &[String]) -> Result<Vec<String>, EngineError> {
        expressions
            .iter()
            .map(|expression| self.eval(expression).map(|result| result.tex))
            .collect()
    }

    /// Render the parsed syntax exactly as supplied, without evaluating it.
    /// Implementations that cannot guarantee this must return an error rather
    /// than silently rendering a different mathematical state.
    fn render_syntax_tex_batch(
        &mut self,
        _expressions: &[String],
    ) -> Result<Vec<String>, EngineError> {
        Err(EngineError::Eval(
            "当前引擎不支持未求值 AST 的 TeX 排版".into(),
        ))
    }

    /// Return matched rule names for step projection; empty when unsupported.
    #[allow(dead_code)] // Step-layer boundary; currently has no consumer.
    fn trace(&mut self, _command: &str) -> Result<Vec<String>, EngineError> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same_ast_projection(expression: Expr, source: &str) {
        crate::input::with_parse_env(|env| {
            let actual = expression.to_canonical_ast(env).unwrap();
            let expected = yacas_rs::parser::parse_expression(env, &format!("{source};"))
                .unwrap()
                .unwrap();
            assert_eq!(
                yacas_rs::printer::infix_print(env, &actual),
                yacas_rs::printer::infix_print(env, &expected)
            );
        });
    }

    #[test]
    fn structured_expr_adapts_infix_relations_lists_and_bodied_calls() {
        assert_same_ast_projection(
            Expr::Call {
                head: "+".into(),
                args: vec![Expr::Symbol("x".into()), Expr::Number("2".into())],
            },
            "x+2",
        );
        assert_same_ast_projection(
            Expr::Call {
                head: "==".into(),
                args: vec![Expr::Symbol("x".into()), Expr::Number("0".into())],
            },
            "x==0",
        );
        assert_same_ast_projection(
            Expr::Call {
                head: "List".into(),
                args: vec![Expr::Symbol("x".into()), Expr::Number("1".into())],
            },
            "{x,1}",
        );
        assert_same_ast_projection(
            Expr::Call {
                head: "D".into(),
                args: vec![
                    Expr::Symbol("x".into()),
                    Expr::Number("2".into()),
                    Expr::Call {
                        head: "Sin".into(),
                        args: vec![Expr::Symbol("x".into())],
                    },
                ],
            },
            "D(x,2)Sin(x)",
        );
    }
}
