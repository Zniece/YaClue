//! Operator-table declaration, precedence, and query commands.

use std::rc::Rc;

use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::operators::Operator;
use crate::value::{LispObject, ObjectKind};

/// ── Operator command family ─────────────────────────────────────────────
///
/// The table key is the key the parser looks up at parse time (the symbol as
/// resident in symtab, matching parser.rs infix_lookup's look_up approach), the
/// same set of keys as the static startup table in
/// operators.rs::register_stdops — loading stdopers.ys must reproduce the
/// static table entry by entry.
/// Precedence upper bound (upstream KMaxPrecedence = 60000; MultiFix family
/// rejects a second argument above this).
const K_MAX_PRECEDENCE: i32 = 60000;

/// One of the operator tables (upstream keeps four LispOperators tables).
enum OpTable {
    Infix,
    Prefix,
    Postfix,
    Bodied,
}

/// Extract an operator name (upstream MultiFix's ARGUMENT(1)->String()): string
/// literals are unquoted, plain atoms keep their name — in stdopers.ys,
/// Bodied(Assert,60000) without quotes is also valid.
fn operator_name(env: &mut Environment, node: &Rc<LispObject>) -> Result<Rc<str>, YacasError> {
    let v = eval(env, node)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    match crate::standard::internal_unstringify(s) {
        Ok(un) => Ok(un.into()),
        Err(_) => Ok(s.clone()),
    }
}

/// Extract a precedence (upstream InternalAsciiToInt +
/// CheckArg(prec<=KMaxPrecedence,2)): a numeric atom or a numeric text atom both
/// work; non-numeric or out-of-range -> InvalidArg.
fn operator_precedence(env: &mut Environment, node: &Rc<LispObject>) -> Result<i32, YacasError> {
    let v = eval(env, node)?;
    let text = match &v.kind {
        ObjectKind::Number(num) => num.string(),
        _ => v.atom_string().ok_or(YacasError::InvalidArg)?.to_string(),
    };
    let prec: i32 = text.parse().map_err(|_| YacasError::InvalidArg)?;
    if prec > K_MAX_PRECEDENCE {
        return Err(YacasError::InvalidArg);
    }
    Ok(prec)
}

/// MultiFix family (Infix/Prefix/Postfix/Bodied).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp MultiFix.
fn multi_fix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    table: OpTable,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let prec = operator_precedence(env, arg(inner, 1)?)?;
    let op = Operator {
        prec,
        left_prec: prec,
        right_prec: prec,
        right_assoc: false,
    };
    let sym = env.symtab.look_up(&name);
    match table {
        OpTable::Infix => {
            env.infix.insert(sym, op);
        }
        OpTable::Prefix => {
            env.prefix.insert(sym, op);
        }
        OpTable::Postfix => {
            env.postfix.insert(sym, op);
        }
        OpTable::Bodied => {
            env.bodied.insert(sym, op);
        }
    }
    Ok(env.true_atom())
}

pub fn cmd_infix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Infix)
}
pub fn cmd_prefix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Prefix)
}
pub fn cmd_postfix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Postfix)
}
pub fn cmd_bodied(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    multi_fix(env, inner, OpTable::Bodied)
}

/// RightAssociative: mark the infix operator as right-associative; a name not in
/// the infix table -> NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRightAssociative.
pub fn cmd_right_associative(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let sym = env.symtab.look_up(&name);
    match env.infix.get_mut(&sym) {
        Some(op) => {
            op.right_assoc = true;
            Ok(env.true_atom())
        }
        None => Err(YacasError::NotAnInfixOperator),
    }
}

/// Left/RightPrecedence: rewrite one side of an infix operator's precedence; a
/// name not in the infix table -> NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispLeftPrecedence /
/// LispRightPrecedence.
fn set_infix_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    right: bool,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let prec = operator_precedence(env, arg(inner, 1)?)?;
    let sym = env.symtab.look_up(&name);
    match env.infix.get_mut(&sym) {
        Some(op) => {
            if right {
                op.right_prec = prec;
            } else {
                op.left_prec = prec;
            }
            Ok(env.true_atom())
        }
        None => Err(YacasError::NotAnInfixOperator),
    }
}

pub fn cmd_left_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    set_infix_precedence(env, inner, false)
}
pub fn cmd_right_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    set_infix_precedence(env, inner, true)
}

/// OpPrecedence family: return the precedence as a numeric atom, for scripts to
/// feed through AsciiToInt (upstream: OpPrecedence("=")=90,
/// OpRightPrecedence("-")=40); a name not in the infix table ->
/// NotAnInfixOperator.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispGetPrecedence /
/// LispGetLeftPrecedence / LispGetRightPrecedence.
enum OpField {
    Precedence,
    LeftPrecedence,
    RightPrecedence,
}

fn get_infix_field(
    env: &mut Environment,
    inner: &Rc<LispObject>,
    field: OpField,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = operator_name(env, arg(inner, 0)?)?;
    let sym = env.symtab.look_up(&name);
    let prec = match env.infix.get(&sym) {
        Some(op) => match field {
            OpField::Precedence => op.prec,
            OpField::LeftPrecedence => op.left_prec,
            OpField::RightPrecedence => op.right_prec,
        },
        None => return Err(YacasError::NotAnInfixOperator),
    };
    let num = crate::value::LispNumber::from_text(prec.to_string());
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Number(num),
    }))
}

pub fn cmd_op_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::Precedence)
}
pub fn cmd_op_left_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::LeftPrecedence)
}
pub fn cmd_op_right_precedence(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    get_infix_field(env, inner, OpField::RightPrecedence)
}

/// IsInfix/IsPrefix/IsPostfix/IsBodied — look the name up in the corresponding
/// operator table.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispIsInFix etc.
fn op_table_has(
    table: &std::collections::HashMap<Rc<str>, crate::operators::Operator>,
    name: &str,
) -> bool {
    table.keys().any(|k| k.as_ref() == name)
}
pub fn cmd_is_infix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    Ok(if op_table_has(&env.infix, &name) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
pub fn cmd_is_prefix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    Ok(if op_table_has(&env.prefix, &name) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
pub fn cmd_is_postfix(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    Ok(if op_table_has(&env.postfix, &name) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
pub fn cmd_is_bodied(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    Ok(if op_table_has(&env.bodied, &name) {
        env.true_atom()
    } else {
        env.false_atom()
    })
}
