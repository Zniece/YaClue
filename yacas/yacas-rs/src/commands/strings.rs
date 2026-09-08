use std::rc::Rc;

use super::{arg, arity_of, int_text_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{spine_kinds, LispObject, ObjectKind};

/// String (See upstream: cyacas/libyacas/src/mathcommands.cpp InternalStringify; Function flag: argument evaluated):
/// returns the argument's text, unconditionally wrapped in quotes. Upstream behavior:
/// String("xx") -> ""xx"", String(aa) -> "aa", String(12) -> "12", and String(f(x))
/// is InvalidArg (sublists have no text form).
pub fn cmd_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(s) => s.to_string(),
        None => match v.number_string() {
            Some(n) => n,
            None => return Err(YacasError::InvalidArg),
        },
    };
    let quoted = format!("\"{text}\"");
    let sym = env.symtab.look_up(&quoted);
    Ok(LispObject::atom(sym))
}

/// Type (See upstream: cyacas/libyacas/src/mathcommands.cpp LispType): a call tree yields its head as a quoted string
/// (Equals(Type(2*x),"*") = True, Equals(Type(Sin(x)),"Sin") = True); non-lists
/// (atoms/numbers) yield the plain atom ""; a non-atomic head (a generic object)
/// also yields "".
pub fn cmd_type(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(_) => "\"\"".to_string(), // Upstream behavior: the non-list branch yields an empty string.
        None => match v.sublist() {
            Some(sub) => match sub.atom_string() {
                Some(s) => format!("\"{s}\""), // Quoted string form (as the stringify lookup produces).
                None => "\"\"".into(), // Non-atomic head -> empty string (same Java branch).
            },
            None => "\"\"".into(), // Numbers/other non-lists -> empty string (Type(5) -> "").
        },
    };
    let sym = env.symtab.look_up(&text);
    Ok(LispObject::atom(sym))
}

/// Atom (See upstream: cyacas/libyacas/src/mathcommands.cpp LispAtom): turns the evaluated argument's text (a number or
/// atom) into an atom node.
pub fn cmd_atom(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    // Upstream behavior: Atom("aa") -> aa; Atom(ConcatStrings("aa","3")) -> aa3 —
    // the evaluated text is unquoted before building the atom. (The Java LispAtomize
    // wraps the text in quotes first, which does not match upstream; upstream is
    // authoritative.)
    let v = eval(env, arg(inner, 0)?)?;
    let text = match v.atom_string() {
        Some(t) => t.to_string(),
        None => v.number_string().unwrap_or_default(),
    };
    let unquoted = crate::standard::internal_unstringify(&text).unwrap_or(&text);
    let sym = env.symtab.look_up(unquoted);
    Ok(LispObject::atom(sym))
}

/// ConcatStrings (See upstream: cyacas/libyacas/src/mathcommands.cpp LispConcatStrings): concatenates the string arguments.
pub fn cmd_concat_strings(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let mut out = String::new();
    for i in 0..arity_of(inner) {
        let v = eval(env, arg(inner, i)?)?;
        let text = match v.atom_string() {
            Some(s) => s.to_string(),
            None => v.number_string().unwrap_or_default(),
        };
        let t = crate::standard::internal_unstringify(&text).unwrap_or(&text);
        out.push_str(t);
    }
    let sym = env.symtab.look_up_stringify(out.as_str());
    Ok(LispObject::atom(sym))
}

/// Concat (See upstream: cyacas/libyacas/src/corefunctions.h LispConcatenate;
/// Function|Variable): every argument must be a list (or a function expression);
/// their contents (each minus its List/function head) are concatenated into a new
/// list: Concat({a},{b}) -> {a,b}. lists.rep's MapArgs body uses
/// UnList(Concat({expr[1]},...)); without this command the argument would stay held
/// and UnList would reject the non-List head.
pub fn cmd_concat(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    let mut kinds: Vec<ObjectKind> = Vec::new();
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let sub = v.sublist().ok_or(YacasError::NotList)?;
        // Splice in this argument's contents (minus its List/function head); an
        // empty list (only a List head) contributes no elements.
        if let Some(first) = sub.next.as_ref() {
            kinds.extend(spine_kinds(first));
        }
    }
    let mut all: Vec<ObjectKind> = Vec::with_capacity(kinds.len() + 1);
    all.push(ObjectKind::Atom(env.symtab.look_up("List")));
    all.extend(kinds);
    let chain = crate::value::build_list(all).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// StringMid'Get (See upstream: cyacas/libyacas/src/mathcommands.cpp YacasStringMidGet):
/// (from, count, str) -> the `count` characters of str starting at 1-based `from`,
/// as a new string.
pub fn cmd_string_mid_get(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    let count = int_text_of(&eval(env, arg(inner, 1)?)?)?;
    if from < 1 {
        return Err(YacasError::InvalidArg);
    }
    let s = eval(env, arg(inner, 2)?)?;
    let text = match s.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let chars: Vec<char> = text.chars().collect();
    let from = (from as usize) - 1;
    let count = count as usize;
    if from + count > chars.len() {
        return Err(YacasError::InvalidArg);
    }
    let sub: String = chars[from..from + count].iter().collect();
    let quoted = format!("\"{sub}\"");
    Ok(LispObject::atom(env.symtab.look_up(&quoted)))
}

/// StringMid'Set (See upstream: cyacas/libyacas/src/mathcommands.cpp YacasStringMidSet):
/// (from, repl, str) -> the contents of str starting at 1-based `from` are replaced
/// by repl, as a new string.
pub fn cmd_string_mid_set(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 3 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let from = int_text_of(&eval(env, arg(inner, 0)?)?)?;
    if from < 1 {
        return Err(YacasError::InvalidArg);
    }
    let repl_node = eval(env, arg(inner, 1)?)?;
    let repl = match repl_node.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let s = eval(env, arg(inner, 2)?)?;
    let text = match s.atom_string() {
        Some(t) => crate::standard::internal_unstringify(t)
            .unwrap_or(t)
            .to_string(),
        None => return Err(YacasError::InvalidArg),
    };
    let chars: Vec<char> = text.chars().collect();
    let from = (from as usize) - 1;
    if from + repl.chars().count() > chars.len() {
        return Err(YacasError::InvalidArg);
    }
    let mut out: Vec<char> = chars.clone();
    for (i, c) in repl.chars().enumerate() {
        out[from + i] = c;
    }
    let res: String = out.iter().collect();
    let quoted = format!("\"{res}\"");
    Ok(LispObject::atom(env.symtab.look_up(&quoted)))
}
