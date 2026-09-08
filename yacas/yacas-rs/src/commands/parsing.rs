use std::rc::Rc;

use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::{spine_kinds, LispObject, ObjectKind};

/// XmlTokenizer()/DefaultTokenizer() (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp LispXmlTokenizer / LispDefaultTokenizer):
/// switch the current tokenizer mode (environment-level flag, synchronized
/// with the active input).
pub fn cmd_xml_tokenizer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.xml_tokenizer.set(true);
    if let Some(Some(t)) = env.input_stack.borrow_mut().last_mut() {
        t.xml = true;
    }
    Ok(env.true_atom())
}
pub fn cmd_default_tokenizer(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    env.xml_tokenizer.set(false);
    if let Some(Some(t)) = env.input_stack.borrow_mut().last_mut() {
        t.xml = false;
    }
    Ok(env.true_atom())
}

/// XmlExplodeTag (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispExplodeTag): an XML tag string -> XmlTag("TAG",{attribute pairs...},
/// "Open"/"Close"/"OpenClose"). Not starting with '<' -> returned unchanged;
/// tag names / attribute names uppercased; an attribute pair =
/// List("NAME","value") (value keeps its quotes); the attribute chain order
/// follows the C++ prepend (later attributes come first).
pub fn cmd_xml_explode_tag(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    if !crate::standard::internal_is_string(s) {
        return Err(YacasError::InvalidArg);
    }
    let body = crate::standard::internal_unstringify(s).unwrap_or(s);
    let chars: Vec<char> = body.chars().collect();
    if chars.first() != Some(&'<') {
        return Ok(v);
    }
    let mut i = 1usize;
    let mut typ = "\"Open\"";
    if chars.get(i) == Some(&'/') {
        typ = "\"Close\"";
        i += 1;
    }
    let mut tag = String::from("\"");
    while i < chars.len() && chars[i].is_alphabetic() {
        tag.push(chars[i].to_ascii_uppercase());
        i += 1;
    }
    tag.push('"');
    let mut pairs: Vec<ObjectKind> = Vec::new();
    loop {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if i >= chars.len() || chars[i] == '>' || chars[i] == '/' {
            break;
        }
        let mut name = String::from("\"");
        while i < chars.len() && chars[i].is_alphabetic() {
            name.push(chars[i].to_ascii_uppercase());
            i += 1;
        }
        name.push('"');
        if chars.get(i) != Some(&'=') {
            return Err(YacasError::InvalidArg);
        }
        i += 1;
        if chars.get(i) != Some(&'"') {
            return Err(YacasError::InvalidArg);
        }
        let mut value = String::from("\"");
        i += 1;
        while i < chars.len() && chars[i] != '"' {
            value.push(chars[i]);
            i += 1;
        }
        value.push('"');
        i += 1;
        let pk = crate::value::build_list(vec![
            ObjectKind::Atom(env.symtab.look_up("List")),
            ObjectKind::Atom(env.symtab.look_up(&name)),
            ObjectKind::Atom(env.symtab.look_up(&value)),
        ])
        .ok_or(YacasError::InvalidArg)?;
        pairs.push(ObjectKind::Sublist(pk));
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
    }
    if chars.get(i) == Some(&'/') {
        typ = "\"OpenClose\"";
        i += 1;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
    }
    pairs.reverse(); // as in C++: prepend to the chain (later attributes come first)
    let mut list_kinds: Vec<ObjectKind> = Vec::with_capacity(pairs.len() + 1);
    list_kinds.push(ObjectKind::Atom(env.symtab.look_up("List")));
    list_kinds.extend(pairs);
    let list_chain = crate::value::build_list(list_kinds).ok_or(YacasError::InvalidArg)?;
    let chain = crate::value::build_list(vec![
        ObjectKind::Atom(env.symtab.look_up("XmlTag")),
        ObjectKind::Atom(env.symtab.look_up(&tag)),
        ObjectKind::Sublist(list_chain),
        ObjectKind::Atom(env.symtab.look_up(typ)),
    ])
    .ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

/// PatchLoad core (see upstream: cyacas/libyacas/src/patcher.cpp PatchLoad):
/// text outside '<?'/'?>' segments is written **verbatim** to the current
/// output; inside a segment, all statements of the script are parsed and
/// evaluated (like DoInternalLoad; output goes to the current output);
/// unclosed segment -> "closing tag not found when patching".
fn patch_process(env: &mut Environment, content: &str) -> Result<(), YacasError> {
    let mut i = 0usize;
    loop {
        let p = content[i..].find("<?").map(|x| x + i);
        let out_end = p.unwrap_or(content.len());
        output_append(env, &content[i..out_end]);
        let pp = match p {
            Some(x) => x,
            None => return Ok(()),
        };
        let q = content[pp + 2..]
            .find("?>")
            .map(|x| x + pp + 2)
            .ok_or_else(|| YacasError::generic("closing tag not found when patching"))?;
        let seg = &content[pp + 2..q];
        let old_file = env.input_file.borrow().clone();
        *env.input_file.borrow_mut() = "String".to_string();
        let r = crate::standard::do_internal_load(env, seg);
        *env.input_file.borrow_mut() = old_file;
        r?;
        i = q + 2;
    }
}

fn output_append(env: &mut Environment, s: &str) {
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    output.last_mut().expect("output").text.push_str(s);
}

/// PatchLoad (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispPatchLoad): evaluates and unquotes the file name, finds it via
/// input_directories; patches the content into the current output; always
/// returns True.
pub fn cmd_patch_load(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let fname = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let path = crate::standard::internal_find_file(env, &fname).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    patch_process(env, &text)?;
    Ok(env.true_atom())
}

/// PatchString (see upstream: cyacas/libyacas/src/mathcommands3.cpp
/// LispPatchString): patches into a fresh buffer -> quoted string (stringify
/// does not escape embedded quotes, as in C++).
pub fn cmd_patch_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let content = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let depth = env.push_output();
    let r = patch_process(env, &content);
    let captured = env.pop_output(depth);
    r?;
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{}\"", captured.text),
    ))
}

/// LispRead/LispReadListed (see upstream:
/// cyacas/libyacas/src/mathcommands3.cpp LispReadLisp / LispReadLispListed and
/// cyacas/libyacas/src/lispparser.cpp LispParser): **purely prefix** parsing
/// (infix not recognized): one token -> atom; '(' -> collect tokens until ')'
/// into a sublist (nested '(' recurses; EOF -> InvalidToken); listed mode
/// prepends a List head before the first element; empty token -> EndOfFile
/// atom. No ';' check.
fn plain_parse(
    env: &mut Environment,
    tok: &mut crate::tokenizer::Tokenizer,
    listed: bool,
) -> Result<Rc<LispObject>, YacasError> {
    let token = tok
        .next_token()
        .map_err(|_| YacasError::generic("Invalid token"))?;
    if token.is_empty() {
        return Ok(LispObject::atom(env.symtab.look_up("EndOfFile")));
    }
    if token != "(" {
        return Ok(crate::value::atom_or_number(&mut env.symtab, &token));
    }
    let mut kinds: Vec<ObjectKind> = Vec::new();
    if listed {
        kinds.push(ObjectKind::Atom(env.symtab.look_up("List")));
    }
    loop {
        let t = tok
            .next_token()
            .map_err(|_| YacasError::generic("Invalid token"))?;
        if t.is_empty() {
            return Err(YacasError::generic("Invalid token"));
        }
        if t == ")" {
            break;
        }
        if t == "(" {
            let sub = plain_parse(env, tok, false)?;
            kinds.push(spine_kinds(&sub).next().expect("node kind"));
        } else {
            kinds.push(ObjectKind::Atom(env.symtab.look_up(&t)));
        }
    }
    let chain = crate::value::build_list(kinds).ok_or(YacasError::InvalidArg)?;
    Ok(Rc::new(LispObject {
        next: None,
        kind: ObjectKind::Sublist(chain),
    }))
}

fn cmd_lisp_read_impl(env: &mut Environment, listed: bool) -> Result<Rc<LispObject>, YacasError> {
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic(
                "LispRead: no active input stream".to_string(),
            ))?
            .take()
            .ok_or(YacasError::Generic(
                "LispRead: input tokenizer missing".to_string(),
            ))?
    };
    let r = plain_parse(env, &mut tok, listed);
    env.input_stack
        .borrow_mut()
        .last_mut()
        .expect("input")
        .replace(tok);
    r
}
pub fn cmd_lisp_read(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, false)
}
pub fn cmd_lisp_read_listed(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    cmd_lisp_read_impl(env, true)
}
