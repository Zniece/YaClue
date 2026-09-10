//! Parser: infix + prefix forms. See upstream
//! `cyacas/libyacas/src/infixparser.cpp` and `lispparser.cpp`.
//!
//! Infix parsing is precedence climbing (`read_expression(depth)`, where
//! `depth` is the minimum precedence still accepted). Special syntax:
//! `a[b]` → `Nth`, `{…}` lists, `[a;b;]` program blocks (every statement
//! must be terminated by `;`), prefix/postfix/bodied operators, and the
//! "longest infix prefix + registered prefix suffix + backtrack" split of
//! greedy symbolic tokens.
//!
//! Chain surgery (`insert_atom`/`combine`) follows contract §1's
//! rebuild-only rule: chains are never mutated in place; nodes are cloned
//! or rebuilt.

use std::rc::Rc;

use crate::env::Environment;
use crate::operators::Operator;
use crate::tokenizer::{is_symbolic, TokenError, Tokenizer};
use crate::value::{atom_or_number, build_list, spine_kinds, spine_refs, LispObject, ObjectKind};

/// Maximum precedence (top-level expressions start here; upstream
/// `KMaxPrecedence`).
pub const K_MAX_PREC: i32 = 60000;

/// Parser resource limits. These are deliberately above normal script and
/// interactive inputs while keeping hostile inputs away from the thread stack
/// and from unbounded token storage.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_PARSE_DEPTH: usize = 256;

#[derive(Debug)]
pub enum ParseError {
    Token(TokenError),
    /// `Fail()`: carries the offending lookahead token.
    InvalidExpression(String),
    /// Concrete syntax error (unclosed bracket / missing statement separator).
    Generic(String),
}

impl From<TokenError> for ParseError {
    fn from(e: TokenError) -> Self {
        ParseError::Token(e)
    }
}

/// Infix parser (combines ParsedObject + InfixParser).
pub struct InfixParser<'a> {
    env: &'a mut Environment,
    tok: &'a mut Tokenizer,
    lookahead: String,
    end_of_file: bool,
    error: bool,
    pub(crate) result: Option<Rc<LispObject>>,
}

/// Parse a single expression (up to `;` or EOF).
pub fn parse_expression(
    env: &mut Environment,
    src: &str,
) -> Result<Option<Rc<LispObject>>, ParseError> {
    if src.len() > MAX_INPUT_BYTES {
        return Err(ParseError::Generic(format!(
            "input exceeds {MAX_INPUT_BYTES} bytes"
        )));
    }
    let mut tok = Tokenizer::new(src);
    parse_one(env, &mut tok)
}

/// Parse one expression from an existing tokenizer (used by file loading):
/// `env` and `tok` are borrowed only for the call, so the caller can then
/// evaluate. Matches the read-parse-eval-per-statement loop of upstream
/// `DoInternalLoad`.
pub fn parse_one(
    env: &mut Environment,
    tok: &mut Tokenizer,
) -> Result<Option<Rc<LispObject>>, ParseError> {
    if tok.input_len() > MAX_INPUT_BYTES {
        return Err(ParseError::Generic(format!(
            "input exceeds {MAX_INPUT_BYTES} bytes"
        )));
    }
    let mut p = InfixParser::new(env, tok);
    p.parse()?;
    Ok(p.result.take())
}

impl<'a> InfixParser<'a> {
    /// Public constructor; reuses `tok` to parse expressions one at a time.
    pub fn new(env: &'a mut Environment, tok: &'a mut Tokenizer) -> Self {
        InfixParser {
            env,
            tok,
            lookahead: String::new(),
            end_of_file: false,
            error: false,
            result: None,
        }
    }

    pub(crate) fn parse(&mut self) -> Result<(), ParseError> {
        self.read_token()?;
        if self.end_of_file {
            self.result = Some(atom_or_number(&mut self.env.symtab, "EndOfFile"));
            return Ok(());
        }
        self.read_expression(K_MAX_PREC, 0)?;
        // After the expression a `;` is required; EOF is also legal (a file's
        // final statement may omit `;`). When the input does end with `;`,
        // the next parse round yields the `EndOfFile` atom.
        if self.lookahead != ";" && !self.end_of_file {
            self.fail()?;
        }
        Ok(())
    }

    fn read_token(&mut self) -> Result<(), ParseError> {
        self.lookahead = self.tok.next_token()?;
        if self.lookahead.is_empty() {
            self.end_of_file = true;
        }
        Ok(())
    }

    fn match_token(&mut self, t: &str) -> Result<(), ParseError> {
        if t != self.lookahead {
            self.fail()?;
        }
        self.read_token()
    }

    fn fail(&mut self) -> Result<(), ParseError> {
        self.error = true;
        Err(ParseError::InvalidExpression(self.lookahead.clone()))
    }

    // Table queries borrow fields individually (symtab mutable, tables
    // read-only) to avoid whole-`&mut self` borrow conflicts.
    fn prefix_lookup(&mut self, s: &str) -> Option<Operator> {
        let sym = self.env.symtab.look_up(s);
        self.env.prefix.get(&sym).cloned()
    }
    fn infix_lookup(&mut self, s: &str) -> Option<Operator> {
        let sym = self.env.symtab.look_up(s);
        self.env.infix.get(&sym).cloned()
    }
    fn postfix_lookup(&mut self, s: &str) -> Option<Operator> {
        let sym = self.env.symtab.look_up(s);
        self.env.postfix.get(&sym).cloned()
    }
    fn bodied_lookup(&mut self, s: &str) -> Option<Operator> {
        let sym = self.env.symtab.look_up(s);
        self.env.bodied.get(&sym).cloned()
    }
    fn prefix_contains(&mut self, s: &str) -> bool {
        let sym = self.env.symtab.look_up(s);
        self.env.prefix.contains_key(&sym)
    }

    fn read_expression(&mut self, precedence: i32, nesting: usize) -> Result<(), ParseError> {
        if nesting > MAX_PARSE_DEPTH {
            return Err(ParseError::Generic(format!(
                "expression exceeds maximum parse depth {MAX_PARSE_DEPTH}"
            )));
        }
        self.read_atom(nesting)?;
        loop {
            // Special case: `a[b]` subscript (lowest precedence) → Nth.
            if self.lookahead == "[" {
                self.match_token("[")?;
                self.read_expression(K_MAX_PREC, nesting + 1)?;
                if self.lookahead != "]" {
                    return Err(ParseError::Generic(format!(
                        "Expecting a ] close bracket for program block, but got {} instead",
                        self.lookahead
                    )));
                }
                self.match_token("]")?;
                self.insert_atom("Nth")?;
                self.combine(2)?;
            } else {
                // Operator split: a token not in the infix table that starts
                // with a symbolic char is shortened to its longest infix
                // prefix whose remaining suffix is a registered prefix
                // operator; the input position is rewound accordingly.
                let la = self.lookahead.clone(); // clone before table lookup to avoid &mut self aliasing
                let op = match self.infix_lookup(&la) {
                    Some(op) => op,
                    None => {
                        let is_sym = self
                            .lookahead
                            .chars()
                            .next()
                            .map(is_symbolic)
                            .unwrap_or(false);
                        if !is_sym {
                            return Ok(());
                        }
                        let origlen = self.lookahead.chars().count();
                        let mut len = origlen;
                        let mut found: Option<Operator> = None;
                        while len > 1 {
                            len -= 1;
                            let head: String = self.lookahead.chars().take(len).collect();
                            if let Some(op) = self.infix_lookup(&head) {
                                let right: String = self.lookahead.chars().skip(len).collect();
                                if self.prefix_contains(&right) {
                                    self.lookahead = head;
                                    let newpos = self.tok.position() - (origlen - len);
                                    self.tok.set_position(newpos);
                                    found = Some(op);
                                    break;
                                }
                            }
                        }
                        match found {
                            Some(op) => op,
                            None => return Ok(()),
                        }
                    }
                };
                if precedence < op.prec {
                    return Ok(());
                }
                // Left-associative: the right operand parses one level
                // higher (upper = prec - 1); right-associative operators
                // (like `^`) recurse at the same level.
                let upper = if op.right_assoc { op.prec } else { op.prec - 1 };
                self.get_other_side(2, upper, nesting)?;
            }
        }
    }

    fn read_atom(&mut self, nesting: usize) -> Result<(), ParseError> {
        // Prefix operator.
        let la = self.lookahead.clone();
        if let Some(op) = self.prefix_lookup(&la) {
            let the_operator = self.lookahead.clone();
            self.match_token(&the_operator)?;
            self.read_expression(op.prec, nesting + 1)?;
            self.insert_atom(&the_operator)?;
            self.combine(1)?;
        }
        // Parentheses.
        else if self.lookahead == "(" {
            self.match_token("(")?;
            self.read_expression(K_MAX_PREC, nesting + 1)?;
            self.match_token(")")?;
        }
        // List {a,b,c}.
        else if self.lookahead == "{" {
            let mut nrargs: usize = 0;
            self.match_token("{")?;
            while self.lookahead != "}" {
                self.read_expression(K_MAX_PREC, nesting + 1)?;
                nrargs += 1;
                if self.lookahead == "," {
                    self.match_token(",")?;
                } else if self.lookahead != "}" {
                    return Err(ParseError::Generic(format!(
                        "Expecting a }} close bracket for program block, but got {} instead",
                        self.lookahead
                    )));
                }
            }
            self.match_token("}")?;
            self.insert_atom("List")?;
            self.combine(nrargs)?;
        }
        // Program block [a;b;].
        else if self.lookahead == "[" {
            let mut nrargs: usize = 0;
            self.match_token("[")?;
            while self.lookahead != "]" {
                self.read_expression(K_MAX_PREC, nesting + 1)?;
                nrargs += 1;
                if self.lookahead == ";" {
                    self.match_token(";")?;
                } else {
                    return Err(ParseError::Generic(format!(
                        "Expecting ; end of statement in program block, but got {} instead",
                        self.lookahead
                    )));
                }
            }
            self.match_token("]")?;
            self.insert_atom("Prog")?;
            self.combine(nrargs)?;
        }
        // Atom, possibly with a call.
        else {
            let the_operator = self.lookahead.clone();
            self.match_token(&the_operator)?;
            let mut nrargs: i64 = -1;
            if self.lookahead == "(" {
                nrargs = 0;
                self.match_token("(")?;
                while self.lookahead != ")" {
                    self.read_expression(K_MAX_PREC, nesting + 1)?;
                    nrargs += 1;
                    if self.lookahead == "," {
                        self.match_token(",")?;
                    } else if self.lookahead != ")" {
                        return Err(ParseError::Generic(format!(
                            "Expecting a ) closing bracket for sub-expression, but got {} instead",
                            self.lookahead
                        )));
                    }
                }
                self.match_token(")")?;
                // Bodied operator: after the closing bracket, read one more
                // expression at bodied precedence as the body — but only if
                // the next token can start an expression (not `;`, `)`, or
                // EOF). A top-level `Subst(x,0,x);` has no trailing body;
                // swallowing `;` would inflate the arity by one.
                if let Some(op) = self.bodied_lookup(&the_operator) {
                    let la = self.lookahead.clone();
                    let is_end = la == ";" || la == ")" || la.is_empty() || la == "EndOfFile";
                    if !is_end {
                        self.read_expression(op.prec, nesting + 1)?;
                        nrargs += 1;
                    }
                }
            }
            self.insert_atom(&the_operator)?;
            if nrargs >= 0 {
                self.combine(nrargs as usize)?;
            }
        }
        // Postfix operators.
        loop {
            let la = self.lookahead.clone();
            if self.postfix_lookup(&la).is_none() {
                break;
            }
            let op = self.lookahead.clone();
            self.insert_atom(&op)?;
            self.match_token(&op)?;
            self.combine(1)?;
        }
        Ok(())
    }

    fn get_other_side(
        &mut self,
        nrargs: usize,
        precedence: i32,
        nesting: usize,
    ) -> Result<(), ParseError> {
        let the_operator = self.lookahead.clone();
        self.match_token(&the_operator)?;
        self.read_expression(precedence, nesting + 1)?;
        self.insert_atom(&the_operator)?;
        self.combine(nrargs)
    }

    /// Prepend an atom (upstream `InsertAtom`), routed through
    /// `atom_or_number`.
    fn insert_atom(&mut self, s: &str) -> Result<(), ParseError> {
        let mut node = atom_or_number(&mut self.env.symtab, s);
        // A freshly built node has refcount 1, so its `next` is writable.
        if let Some(n) = Rc::get_mut(&mut node) {
            n.next = self.result.take();
        } else {
            return Err(ParseError::Generic("insert_atom: node is shared".into()));
        }
        self.result = Some(node);
        Ok(())
    }

    /// Chain surgery (upstream `Combine`): take `[op, argN..arg1, rest..]`,
    /// wrap the head plus the first `nargs` args (reversed back into
    /// argument order) into a sublist, and attach `rest` as the sublist
    /// node's `next`.
    fn combine(&mut self, nargs: usize) -> Result<(), ParseError> {
        let chain = self
            .result
            .take()
            .ok_or_else(|| ParseError::Generic("combine: result chain is empty".into()))?;
        let refs: Vec<Rc<LispObject>> = spine_refs(&chain).cloned().collect();
        if refs.len() < nargs + 1 {
            return Err(ParseError::Generic(format!(
                "combine: need {} items, found {}",
                nargs + 1,
                refs.len()
            )));
        }
        let rest = refs.get(nargs + 1).cloned();
        let mut kinds: Vec<ObjectKind> = Vec::with_capacity(nargs + 1);
        kinds.push(spine_kinds(&refs[0]).next().expect("combine head"));
        for r in refs[1..=nargs].iter().rev() {
            kinds.push(spine_kinds(r).next().expect("combine arg"));
        }
        let inner = build_list(kinds).expect("combine inner");
        self.result = Some(Rc::new(LispObject {
            next: rest,
            kind: ObjectKind::Sublist(inner),
        }));
        Ok(())
    }
}

/// Prefix-form parser (`(...)` → sublist).
pub struct LispParser<'a> {
    env: &'a mut Environment,
    tok: Tokenizer,
}

impl<'a> LispParser<'a> {
    pub fn new(env: &'a mut Environment, src: &str) -> Self {
        LispParser {
            env,
            tok: Tokenizer::new(src),
        }
    }

    /// Parse one form; EOF yields the `EndOfFile` atom.
    pub fn parse(&mut self) -> Result<Rc<LispObject>, ParseError> {
        if self.tok.input_len() > MAX_INPUT_BYTES {
            return Err(ParseError::Generic(format!(
                "input exceeds {MAX_INPUT_BYTES} bytes"
            )));
        }
        let token = self.tok.next_token()?;
        if token.is_empty() {
            return Ok(atom_or_number(&mut self.env.symtab, "EndOfFile"));
        }
        self.parse_atom(&token, 0)
    }

    fn parse_atom(&mut self, token: &str, nesting: usize) -> Result<Rc<LispObject>, ParseError> {
        if nesting > MAX_PARSE_DEPTH {
            return Err(ParseError::Generic(format!(
                "expression exceeds maximum parse depth {MAX_PARSE_DEPTH}"
            )));
        }
        if token.is_empty() {
            return Err(ParseError::InvalidExpression(String::new()));
        }
        if token == "(" {
            let sub = self.parse_list(nesting + 1)?;
            return Ok(Rc::new(LispObject {
                next: None,
                kind: ObjectKind::Sublist(sub),
            }));
        }
        Ok(atom_or_number(&mut self.env.symtab, token))
    }

    fn parse_list(&mut self, nesting: usize) -> Result<Rc<LispObject>, ParseError> {
        let mut kinds: Vec<ObjectKind> = Vec::new();
        loop {
            let token = self.tok.next_token()?;
            if token.is_empty() {
                return Err(ParseError::InvalidExpression(String::new()));
            }
            if token == ")" {
                break;
            }
            let node = self.parse_atom(&token, nesting)?;
            kinds.push(spine_kinds(&node).next().expect("parse_list item"));
        }
        build_list(kinds).ok_or_else(|| ParseError::Generic("parse_list: empty sublist".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::full_form;

    fn ff(env: &mut Environment, src: &str) -> String {
        // Inputs are terminated with `;` like console input.
        let tree = parse_expression(env, &format!("{src};"))
            .expect("parse")
            .expect("non-empty");
        full_form(&tree)
    }

    #[test]
    fn infix_flat() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "aa+bb"), "(+ aa bb )");
        assert_eq!(ff(&mut env, "aa/bb"), "(/ aa bb )");
        assert_eq!(ff(&mut env, "(aa)"), "aa ");
    }

    #[test]
    fn infix_precedence() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "aa+bb*cc"), "(+ aa \n    (* bb cc ))");
        assert_eq!(ff(&mut env, "aa-bb-cc"), "(- \n    (- aa bb )cc )");
        assert_eq!(ff(&mut env, "aa^bb^cc"), "(^ aa \n    (^ bb cc ))");
        assert_eq!(
            ff(&mut env, "aa^bb^cc^dd"),
            "(^ aa \n    (^ bb \n      (^ cc dd )))"
        );
    }

    #[test]
    fn prefix_and_postfix() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "-aa"), "(- aa )");
        assert_eq!(ff(&mut env, "-aa^2"), "(- \n    (^ aa 2 ))");
        assert_eq!(ff(&mut env, "aa!"), "(! aa )");
        assert_eq!(ff(&mut env, "aa! + bb"), "(+ \n    (! aa )bb )");
    }

    #[test]
    fn calls_lists_nth() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "ff(aa,bb,cc)"), "(ff aa bb cc )");
        assert_eq!(ff(&mut env, "aa[bb][cc]"), "(Nth \n    (Nth aa bb )cc )");
        assert_eq!(
            ff(&mut env, "{aa,{bb,cc}}"),
            "(List aa \n    (List bb cc ))"
        );
        assert_eq!(ff(&mut env, "{}"), "(List )");
        assert_eq!(ff(&mut env, "aa[1]"), "(Nth aa 1 )");
    }

    #[test]
    fn operator_split_and_underscore() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "aa<-bb"), "(<- aa bb )");
        assert_eq!(ff(&mut env, "aa_1"), "(_ aa 1 )");
    }

    #[test]
    fn comments_skipped() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "aa + /* comment */ bb"), "(+ aa bb )");
    }

    #[test]
    fn number_and_string_atoms() {
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "1.5"), "1.5 ");
        assert_eq!(ff(&mut env, "2.5e-3"), "2.5e-3 ");
        assert_eq!(ff(&mut env, "\"hello\""), "\"hello\" ");
        assert_eq!(ff(&mut env, "\"a\\\"b\""), "\"a\"b\" ");
    }

    #[test]
    fn bodied_operator() {
        // Bodied operators read the body after `)` at bodied precedence:
        // `if` has bodied prec 5 (does not swallow `+`, prec 70), while
        // `D`/`For` use KMax and swallow everything.
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "if(aa) bb+cc"), "(+ \n    (if aa bb )cc )");
        assert_eq!(ff(&mut env, "D(aa) bb+cc"), "(D aa \n    (+ bb cc ))");
    }

    #[test]
    fn prog_block() {
        // Program blocks require `;` after every statement (including the
        // last, before `]`).
        let mut env = Environment::new();
        assert_eq!(ff(&mut env, "[aa;bb;cc;]"), "(Prog aa bb cc )");
        let mut env2 = Environment::new();
        assert!(
            parse_expression(&mut env2, "[aa;bb;cc];").is_err(),
            "missing final ; must error"
        );
    }

    #[test]
    fn rejects_excessive_input_before_tokenizing() {
        let mut env = Environment::new();
        let source = "a".repeat(MAX_INPUT_BYTES + 1);
        assert!(matches!(
            parse_expression(&mut env, &source),
            Err(ParseError::Generic(message)) if message.contains("input exceeds")
        ));
    }

    #[test]
    fn rejects_excessive_infix_nesting() {
        let mut env = Environment::new();
        let source = format!(
            "{}a{}",
            "(".repeat(MAX_PARSE_DEPTH + 1),
            ")".repeat(MAX_PARSE_DEPTH + 1)
        );
        assert!(matches!(
            parse_expression(&mut env, &source),
            Err(ParseError::Generic(message)) if message.contains("maximum parse depth")
        ));
    }

    #[test]
    fn rejects_excessive_prefix_nesting() {
        let mut env = Environment::new();
        let source = format!(
            "{}a{}",
            "(".repeat(MAX_PARSE_DEPTH + 1),
            ")".repeat(MAX_PARSE_DEPTH + 1)
        );
        assert!(matches!(
            LispParser::new(&mut env, &source).parse(),
            Err(ParseError::Generic(message)) if message.contains("maximum parse depth")
        ));
    }
}
