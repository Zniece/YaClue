//! FullForm printer. See upstream `cyacas/libyacas/src/lispparser.cpp`
//! (`LispPrinter`).
//!
//! Atoms (including numbers and strings) print as "text + space"; sublists
//! print as `(` + contents + `)`, with a nested sublist at position ≥ 1 of
//! its parent chain indented (newline + 2×depth spaces); generics print as
//! `[GenericObject]`; an empty sublist prints as `()`.

use std::rc::Rc;

use crate::value::{ObjectKind, LispObject};

/// Print an expression in FullForm.
pub fn full_form(expr: &Rc<LispObject>) -> String {
    let mut out = String::new();
    print_expression(Some(expr), &mut out, 0);
    out
}

fn print_expression(mut cur: Option<&Rc<LispObject>>, out: &mut String, depth: usize) {
    let mut item: usize = 0;
    while let Some(n) = cur {
        match &n.kind {
            ObjectKind::Atom(s) => {
                out.push_str(s);
                out.push(' ');
            }
            ObjectKind::Number(num) => {
                out.push_str(&num.string());
                out.push(' ');
            }
            ObjectKind::Sublist(inner) => {
                if item != 0 {
                    indent(out, depth + 1);
                }
                out.push('(');
                print_expression(Some(inner), out, depth + 1);
                out.push(')');
                item = 0;
            }
            ObjectKind::Generic(_) => out.push_str("[GenericObject]"),
        }
        cur = n.next.as_ref();
        item += 1;
    }
}

fn indent(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str("  ");
    }
}

// ==================================================================
// Infix printer (REPL result display). See upstream
// `cyacas/libyacas/src/infixprinter.cpp`.
// ==================================================================

use crate::env::Environment;
use crate::tokenizer::{is_alpha, is_symbolic};

const K_MAX_PREC: i32 = 60000;

/// Output buffer carrying the last character of the previous token, which
/// drives the spacing rules of `write_token`.
pub struct InfixOut {
    pub text: String,
    prev_last_char: char,
}

impl Default for InfixOut {
    fn default() -> Self {
        Self::new()
    }
}

impl InfixOut {
    pub fn new() -> Self {
        InfixOut { text: String::new(), prev_last_char: '\0' }
    }
    fn write_token(&mut self, s: &str) {
        let first = s.chars().next().unwrap_or('\0');
        let alnum = |c: char| is_alpha(c) || c.is_ascii_digit();
        let need_space = (alnum(self.prev_last_char) && (alnum(first) || first == '_'))
            || (is_symbolic(self.prev_last_char) && is_symbolic(first));
        if need_space {
            self.text.push(' ');
        }
        self.text.push_str(s);
        self.prev_last_char = s.chars().next_back().unwrap_or('\0');
    }
}

/// Print an expression starting at maximum precedence.
pub fn infix_print(env: &Environment, expr: &Rc<LispObject>) -> String {
    let mut out = InfixOut::new();
    infix_expression(env, expr, &mut out, K_MAX_PREC);
    out.text
}

/// Session-precision variant: numbers render via the `ToString` path, so
/// decimal digits beyond the builtin precision are cut (see
/// `LispNumber::string_at`).
pub fn infix_print_at(env: &Environment, expr: &Rc<LispObject>) -> String {
    let mut out = InfixOut::new();
    infix_expression_at(env, expr, &mut out, K_MAX_PREC);
    out.text
}

/// Print into a *shared* output buffer: spacing decisions use
/// `buf.prev_last_char` across calls (consecutive `Write`s share printer
/// state). A separating space is inserted when both boundary characters are
/// alphanumeric; quotes do not trigger one.
pub fn infix_print_into(env: &Environment, expr: &Rc<LispObject>, buf: &mut crate::env::OutputBuffer) {
    let rendered = infix_print_at(env, expr);
    let first = rendered.chars().next().unwrap_or('\0');
    let alnum = |c: char| is_alpha(c) || c.is_ascii_digit();
    let prev = buf.prev_last_char;
    let need_space = !buf.text.is_empty()
        && prev != '\0'
        && alnum(prev) && (alnum(first) || first == '_');
    if need_space {
        buf.text.push(' ');
    }
    buf.text.push_str(&rendered);
    buf.prev_last_char = rendered.chars().next_back().unwrap_or(prev);
}

/// Print a single node.
fn infix_expression(env: &Environment, expr: &Rc<LispObject>, out: &mut InfixOut, prec: i32) {
    let printable = match &expr.kind {
        ObjectKind::Atom(sym) => Some(sym.to_string()),
        ObjectKind::Number(n) => Some(n.string()),
        _ => None,
    };
    print_atomish(env, expr, printable, out, prec);
}

/// Session-precision variant (numbers via `string_at`).
fn infix_expression_at(env: &Environment, expr: &Rc<LispObject>, out: &mut InfixOut, prec: i32) {
    let printable = match &expr.kind {
        ObjectKind::Atom(sym) => Some(sym.to_string()),
        ObjectKind::Number(n) => Some(n.string_at(env.precision())),
        _ => None,
    };
    print_atomish(env, expr, printable, out, prec);
}

fn print_atomish(
    env: &Environment,
    expr: &Rc<LispObject>,
    printable: Option<String>,
    out: &mut InfixOut,
    prec: i32,
) {
    let _ = env;
    if let Some(s) = printable {
        // Negative literals get parentheses below top level.
        let mut bracket = false;
        if prec < K_MAX_PREC {
            let mut chars = s.chars();
            if chars.next() == Some('-') {
                if let Some(c) = chars.next() {
                    if c.is_ascii_digit() || c == '.' {
                        bracket = true;
                    }
                }
            }
        }
        if bracket {
            out.write_token("(");
        }
        out.write_token(&s);
        if bracket {
            out.write_token(")");
        }
        return;
    }
    // Generics: Association → Association(<ToList>), Array →
    // Array({e1,e2,...}) (each slot at max precedence); other generics print
    // their (quoted) type name.
    if let ObjectKind::Generic(g) = &expr.kind {
        if let Some(a) = g.downcast_assoc() {
            out.write_token("Association");
            out.write_token("(");
            let entries: Vec<(Rc<LispObject>, Rc<LispObject>)> =
                a.pairs.borrow().iter().cloned().collect();
            // Entries are sorted by key order, like the upstream ToList view.
            let mut sorted = entries;
            sorted.sort_by(|x, y| {
                if crate::standard::total_less(env, &x.0, &y.0) { std::cmp::Ordering::Less }
                else if crate::standard::total_less(env, &y.0, &x.0) { std::cmp::Ordering::Greater }
                else { std::cmp::Ordering::Equal }
            });
            // Build the {k1,v1},{k2,v2}… list, then print it.
            let mut all = vec![ObjectKind::Atom(env.symtab.get("List").expect("List").clone())];

            for (k, v) in &sorted {
                let mut pair = vec![ObjectKind::Atom(env.symtab.get("List").expect("List").clone())];

                pair.push(crate::value::spine_kinds(k).next().expect("k"));
                pair.push(crate::value::spine_kinds(v).next().expect("v"));
                all.push(ObjectKind::Sublist(crate::value::build_list(pair).expect("pair")));
            }
            let tolist = crate::value::build_list(all).expect("tolist");
            let list_node = Rc::new(LispObject { next: None, kind: ObjectKind::Sublist(tolist) });
            infix_expression(env, &list_node, out, K_MAX_PREC);
            out.write_token(")");
            return;
        }
        if let Some(arr) = g.downcast_array() {
            out.write_token("Array");
            out.write_token("(");
            out.write_token("{");
            let slots = arr.slots.borrow();
            let n = slots.len();
            for (i, slot) in slots.iter().enumerate() {
                if let Some(v) = slot {
                    infix_expression(env, v, out, K_MAX_PREC);
                }
                if i + 1 != n {
                    out.write_token(",");
                }
            }
            out.write_token("}");
            out.write_token(")");
            return;
        }
        out.write_token(g.type_name());
        return;
    }
    let sub = match expr.sublist() {
        Some(sub) => sub,
        None => return,
    };
    // Empty sublist prints as "( )".
    if sub.atom_string().is_none() && sub.number_string().is_none() && sub.sublist().is_none() {
        out.write_token("( )");
        return;
    }
    let length = crate::standard::internal_list_length(sub);
    let head_str = match sub.atom_string() {
        Some(s) => s.to_string(),
        None => String::new(),
    };
    let head_rc = env.symtab.get(&head_str);
    // Table lookup: prefix/postfix need arity 2, infix arity 3.
    let prefix = if length == 2 { head_rc.and_then(|r| env.prefix.get(r).cloned()) } else { None };
    let infix = if length == 3 { head_rc.and_then(|r| env.infix.get(r).cloned()) } else { None };
    let postfix = if length == 2 { head_rc.and_then(|r| env.postfix.get(r).cloned()) } else { None };
    let bodied = head_rc.and_then(|r| env.bodied.get(r).cloned());
    let op = prefix.as_ref().or(postfix.as_ref()).or(infix.as_ref());
    if let Some(op) = op {
        // Operand positions: infix → left+right, postfix → left only,
        // prefix → right only.
        let (left, right) = if infix.is_some() {
            (
                sub.next.as_ref(),
                sub.next.as_ref().and_then(|n| n.next.as_ref()),
            )
        } else if prefix.is_some() {
            (None, sub.next.as_ref())
        } else {
            (sub.next.as_ref(), None) // postfix: left operand only
        };
        if prec < op.prec {
            out.write_token("(");
        }
        if let Some(l) = left {
            infix_expression(env, l, out, op.left_prec);
        }
        out.write_token(&head_str);
        if let Some(r) = right {
            infix_expression(env, r, out, op.right_prec);
        }
        if prec < op.prec {
            out.write_token(")");
        }
        return;
    }
    // Non-operators: List / Prog / Nth / ordinary functions.
    let list_name = env.symtab.get("List");
    let prog_name = env.symtab.get("Prog");
    let nth_name = env.symtab.get("Nth");
    let mut iter = sub.next.as_ref();
    let same_as = |a: &Option<&Rc<str>>, b: &Option<&Rc<str>>| match (a, b) {
        (Some(x), Some(y)) => Rc::ptr_eq(x, y),
        _ => false,
    };
    if same_as(&head_rc, &list_name) {
        out.write_token("{");
        let mut first = true;
        while let Some(n) = iter {
            if !first {
                out.write_token(",");
            }
            infix_expression(env, n, out, K_MAX_PREC);
            iter = n.next.as_ref();
            first = false;
        }
        out.write_token("}");
        return;
    }
    if same_as(&head_rc, &prog_name) {
        out.write_token("[");
        while let Some(n) = iter {
            infix_expression(env, n, out, K_MAX_PREC);
            iter = n.next.as_ref();
            out.write_token(";");
        }
        out.write_token("]");
        return;
    }
    if same_as(&head_rc, &nth_name) {
        if let Some(l) = iter {
            infix_expression(env, l, out, 0);
            iter = l.next.as_ref();
        }
        out.write_token("[");
        if let Some(r) = iter {
            infix_expression(env, r, out, K_MAX_PREC);
        }
        out.write_token("]");
        return;
    }
    // Ordinary function (with bodied body).
    let mut bracket = false;
    if let Some(b) = bodied.as_ref() {
        if prec < b.prec {
            bracket = true;
        }
    }
    if bracket {
        out.write_token("(");
    }
    out.write_token(&head_str);
    out.write_token("(");
    let mut count = 0usize;
    let mut c = iter;
    while let Some(n) = c {
        count += 1;
        c = n.next.as_ref();
    }
    let mut nr = count;
    if bodied.is_some() {
        nr = nr.saturating_sub(1);
    }
    let mut it = iter;
    while nr > 0 {
        let n = it.expect("args");
        infix_expression(env, n, out, K_MAX_PREC);
        it = n.next.as_ref();
        nr -= 1;
        if nr > 0 {
            out.write_token(",");
        }
    }
    out.write_token(")");
    if let Some(n) = it {
        if let Some(b) = bodied.as_ref() {
            infix_expression(env, n, out, b.prec);
        }
    }
    if bracket {
        out.write_token(")");
    }
}

#[cfg(test)]
mod tests {
    use crate::env::Environment;
    use crate::parser::parse_expression;
    use crate::printer::full_form;

    fn ff(src: &str) -> String {
        let mut env = Environment::new();
        let tree = parse_expression(&mut env, &format!("{src};"))
            .expect("parse")
            .expect("non-empty");
        full_form(&tree)
    }

    #[test]
    fn flat_atoms() {
        assert_eq!(ff("aa"), "aa ");
        assert_eq!(ff("1.5"), "1.5 ");
        assert_eq!(ff("\"hello\""), "\"hello\" ");
    }

    #[test]
    fn nested_indent() {
        assert_eq!(ff("aa+bb*cc"), "(+ aa \n    (* bb cc ))");
        assert_eq!(
            ff("aa^bb^cc^dd"),
            "(^ aa \n    (^ bb \n      (^ cc dd )))"
        );
    }
}
