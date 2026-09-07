//! Operator tables. See upstream `cyacas/libyacas/include/yacas/lispoperator.h`
//! and the startup registration in `scripts/stdopers.ys`.
//!
//! Four tables (prefix/infix/postfix/bodied), consulted live by the parser
//! (tables may grow while scripts load). The state after
//! [`register_stdops`] matches the upstream engine's post-startup state.

use std::collections::HashMap;
use std::rc::Rc;

/// Operator attributes: a unified precedence plus independent left/right
/// precedences and a right-associativity flag.
#[derive(Debug, Clone)]
pub struct Operator {
    pub prec: i32,
    pub left_prec: i32,
    pub right_prec: i32,
    pub right_assoc: bool,
}

pub type OperatorTable = HashMap<Rc<str>, Operator>;

/// Startup operator registration; fills the four tables of `Environment`.
pub fn register_stdops(e: &mut crate::env::Environment) {
    let infix = |t: &mut crate::env::Environment, name: &str, prec: i32| {
        let sym = t.symtab.look_up(name);
        t.infix.insert(
            sym,
            Operator {
                prec,
                left_prec: prec,
                right_prec: prec,
                right_assoc: false,
            },
        );
    };
    let prefix = |t: &mut crate::env::Environment, name: &str, prec: i32| {
        let sym = t.symtab.look_up(name);
        t.prefix.insert(
            sym,
            Operator {
                prec,
                left_prec: prec,
                right_prec: prec,
                right_assoc: false,
            },
        );
    };
    let postfix = |t: &mut crate::env::Environment, name: &str, prec: i32| {
        let sym = t.symtab.look_up(name);
        t.postfix.insert(
            sym,
            Operator {
                prec,
                left_prec: prec,
                right_prec: prec,
                right_assoc: false,
            },
        );
    };
    let bodied = |t: &mut crate::env::Environment, name: &str, prec: i32| {
        let sym = t.symtab.look_up(name);
        t.bodied.insert(
            sym,
            Operator {
                prec,
                left_prec: prec,
                right_prec: prec,
                right_assoc: false,
            },
        );
    };
    let right_assoc = |t: &mut crate::env::Environment, name: &str| {
        if let Some(op) = t.infix.get_mut(&t.symtab.look_up(name)) {
            op.right_assoc = true;
        }
    };
    let right_prec = |t: &mut crate::env::Environment, name: &str, rp: i32| {
        if let Some(op) = t.infix.get_mut(&t.symtab.look_up(name)) {
            op.right_prec = rp;
        }
    };

    // scripts/stdopers.ys
    infix(e, "=", 90);
    infix(e, "And", 1000);
    right_assoc(e, "And");
    infix(e, "Or", 1010);
    prefix(e, "Not", 100);
    infix(e, "<", 90);
    infix(e, ">", 90);
    infix(e, "<=", 90);
    infix(e, ">=", 90);
    infix(e, "!=", 90);
    infix(e, ":=", 10000);
    right_assoc(e, ":=");
    infix(e, "+", 70);
    infix(e, "-", 70);
    right_prec(e, "-", 40);
    infix(e, "/", 30);
    infix(e, "*", 40);
    infix(e, "^", 20);
    right_assoc(e, "^");
    prefix(e, "+", 50);
    prefix(e, "-", 50);
    bodied(e, "For", 60000);
    bodied(e, "Until", 60000);
    postfix(e, "++", 5);
    postfix(e, "--", 5);
    bodied(e, "ForEach", 60000);
    infix(e, "<<", 10);
    infix(e, ">>", 10);
    bodied(e, "D", 60000);
    bodied(e, "Deriv", 60000);
    infix(e, "X", 30);
    infix(e, ".", 30);
    infix(e, "o", 30);
    postfix(e, "!", 30);
    postfix(e, "!!", 30);
    infix(e, "***", 50);
    bodied(e, "Integrate", 60000);
    bodied(e, "NIntegrate", 60000);
    bodied(e, "Limit", 60000);
    infix(e, ":", 70);
    right_assoc(e, ":");
    infix(e, "@", 600);
    infix(e, "/@", 600);
    infix(e, "..", 600);
    bodied(e, "Taylor", 60000);
    bodied(e, "Taylor1", 60000);
    bodied(e, "Taylor2", 60000);
    bodied(e, "Taylor3", 60000);
    bodied(e, "InverseTaylor", 60000);
    infix(e, "<--", 10000);
    infix(e, "#", 9900);
    bodied(e, "TSum", 60000);
    bodied(e, "TExplicitSum", 60000);
    bodied(e, "TD", 5);
    infix(e, "==", 90);
    infix(e, "!==", 90);
    infix(e, "=>", 10000);
    bodied(e, "if", 5);
    infix(e, "else", 60000);
    right_assoc(e, "else");
    infix(e, "&", 50);
    infix(e, "|", 50);
    infix(e, "%", 50);
    infix(e, "/:", 20000);
    infix(e, "/::", 20000);
    infix(e, "<-", 10000);
    // <> and <=> inherit the precedence of "=" (OpPrecedence("=") == 90)
    infix(e, "<>", 90);
    infix(e, "<=>", 90);
    infix(e, "Where", 11000);
    infix(e, "AddTo", 2000);
    bodied(e, "Function", 60000);
    bodied(e, "Macro", 60000);
    bodied(e, "Assert", 60000);
    bodied(e, "Defun", 0);
    infix(e, "<->", 90);
    infix(e, "->", 90);

    // Core-registration operators: prefix backquote/@/_ and infix _; bodied
    // control structures and the Rule family.
    prefix(e, "`", 0);
    prefix(e, "@", 0);
    prefix(e, "_", 0);
    infix(e, "_", 0);
    const KMAX: i32 = 60000;
    bodied(e, "While", KMAX);
    bodied(e, "Rule", KMAX);
    bodied(e, "MacroRule", KMAX);
    bodied(e, "RulePattern", KMAX);
    bodied(e, "MacroRulePattern", KMAX);
    bodied(e, "FromFile", KMAX);
    bodied(e, "FromString", KMAX);
    bodied(e, "ToFile", KMAX);
    bodied(e, "ToString", KMAX);
    bodied(e, "ToStdout", KMAX);
    bodied(e, "TraceRule", KMAX);
    bodied(e, "Subst", KMAX);
    bodied(e, "LocalSymbols", KMAX);
    bodied(e, "BackQuote", KMAX);
}
