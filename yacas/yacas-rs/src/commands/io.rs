//! Input, output, filesystem, process, and secure-scope commands.

use std::rc::Rc;

use super::{arg, arity_of};
use crate::env::Environment;
use crate::errors::YacasError;
use crate::evaluator::eval;
use crate::value::LispObject;

// ======================= Output stream family (Write/WriteString/ToString) =======================
/// Write — evaluates the arguments one by one and prints them to the **current
/// output buffer** with the printer (the space rule is shared across calls,
/// upstream InfixPrinter::iPrevLastChar); returns True. `Write(a,b)` puts a
/// space between arguments ("1 2").
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispWrite (Function|Variable).
pub fn cmd_write(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    // Upstream: with Variable arity the remaining arguments are packed into a
    // List; Rust commands receive all arguments, so evaluate and print one by
    // one. Each argument is evaluated then written to the buffer (no RefCell
    // borrow held across eval, avoiding aliasing conflicts).
    for i in 0..n {
        let v = eval(env, arg(inner, i)?)?;
        let mut output = env.output_stack.borrow_mut();
        if output.is_empty() {
            output.push(crate::env::OutputBuffer::default());
        }
        let buf = output.last_mut().expect("output");
        crate::printer::infix_print_into(env, &v, buf);
        drop(output);
    }
    Ok(env.true_atom())
}

/// WriteString — the argument must be a string atom; its unquoted body is written
/// **verbatim** to the current output (no spaces, contents not evaluated) and the
/// printer's last character is updated. Returns True. Used heavily by io.rep
/// (WriteString("...")).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispWriteString (Function|Fixed).
pub fn cmd_write_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Function flag: the argument is already evaluated (eval here for the value,
    // consistent with other Function commands).
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    if !crate::standard::internal_is_string(s) {
        return Err(YacasError::InvalidArg);
    }
    let body = crate::standard::internal_unstringify(s).expect("unquote string");
    let mut output = env.output_stack.borrow_mut();
    if output.is_empty() {
        output.push(crate::env::OutputBuffer::default());
    }
    let buf = output.last_mut().expect("output");
    buf.text.push_str(body);
    buf.prev_last_char = body.chars().next_back().unwrap_or(buf.prev_last_char);
    drop(output);
    Ok(env.true_atom())
}

/// ToString — call form `ToString()[body]` (bodied, Macro|Fixed).
/// Pushes a new output buffer capturing the Write/WriteString output produced
/// while the body evaluates; afterwards pops the stack and returns the captured
/// text wrapped as a **quoted** string atom (upstream: stringify(os.str())).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToString.
pub fn cmd_to_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    // bodied: arguments are [head, body] (body is a Prog block or a single
    // expression); Macro does not pre-evaluate arguments.
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let depth = env.push_output();
    // Macro: the body is not evaluated by the evaluator; evaluate it here
    // (capturing its Write/WriteString output).
    let body_result = eval(env, arg(inner, n - 1)?);
    let captured = env.pop_output(depth);
    match body_result {
        Ok(_) => {
            let quoted = format!("\"{}\"", captured.text);
            Ok(crate::value::make_atom(&mut env.symtab, &quoted))
        }
        Err(e) => Err(e),
    }
}

/// ToFile — `ToFile("name")body` (Macro|Fixed + bodied). The first argument
/// (file name) is evaluated and unquoted; pushes an output buffer bound to the
/// file, evaluates the body (its Write/WriteString output accumulates in the
/// buffer), and on pop **truncates and writes the file** (upstream LispLocalFile
/// with ios_base::out: overwrites every time); returns the body's result.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToFile.
pub fn cmd_to_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let name = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let depth = env.push_output_to(Some(name));
    let body_result = eval(env, arg(inner, n - 1)?);
    env.pop_output(depth); // flush to disk (truncating write)
    body_result
}

/// ToStdout — `ToStdout()body` (Macro|Fixed + bodied). The body's output is
/// forced to the **initial stdout** (bypassing ToString/ToFile capture; upstream
/// LispLocalOutput(*iInitialOutput)). In the library scenario there is no real
/// stdout, so this is equivalent to a discard sink: push a discard buffer ->
/// eval body -> pop and drop (text neither hits disk nor any capture). Returns
/// the body's result.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispToStdout.
pub fn cmd_to_stdout(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let depth = env.push_output();
    let body_result = eval(env, arg(inner, n - 1)?);
    env.pop_output(depth);
    body_result
}

// ======================= SystemCall / SystemName =======================
/// SystemCall — the argument must be a string; it is unquoted and executed via
/// the shell (system() semantics); exit code == 0 -> True, otherwise False.
/// CheckSecure: raises SecurityBreach when env.secure. Required by the
/// mysql/rm toolchains in io.rep/html.rep.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSystemCall (Function|Fixed).
pub fn cmd_system_call(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let v = eval(env, arg(inner, 0)?)?;
    let s = v.atom_string().ok_or(YacasError::InvalidArg)?;
    let cmd = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    // system() semantics: executed via sh -c; exit code 0 -> True.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .status();
    match status {
        Ok(st) => {
            if st.success() {
                Ok(env.true_atom())
            } else {
                Ok(env.false_atom())
            }
        }
        Err(_) => Ok(env.false_atom()),
    }
}

/// SystemName — returns the platform name as a quoted string
/// (Linux/MacOSX/Windows/Unknown).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSystemName (Function|Fixed, arity 0).
pub fn cmd_system_name(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name = if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "MacOSX"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    };
    Ok(crate::value::make_atom(
        &mut env.symtab,
        &format!("\"{name}\""),
    ))
}

/// TmpFile — mkstemp semantics: creates a unique temporary file (success only if
/// it does not exist, looping over random suffixes) and returns the **quoted**
/// path string (upstream oracle: "/tmp/yacas-uuedFA"). CheckSecure. Used by the
/// plots backends mostly for intermediate control/data files. The file is kept
/// after creation (consumers use ToFile/FromFile on it).
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispTmpFile (Function|Fixed, arity 0).
pub fn cmd_tmp_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    use std::io::Write;
    // mkstemp("/tmp/yacas-XXXXXX") semantics: 6 random chars; create_new guarantees uniqueness.
    let dir = "/tmp";
    let chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = 0u64;
    // Seed from time + address (no extra dependencies needed)
    rng ^= std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    rng ^= (env as *const Environment) as u64;
    for _attempt in 0..100 {
        let mut suffix = String::new();
        for _ in 0..6 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = ((rng >> 33) % chars.len() as u64) as usize;
            suffix.push(chars[idx] as char);
        }
        let path = format!("{dir}/yacas-{suffix}");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                let _ = f.write_all(b"");
                drop(f);
                return Ok(crate::value::make_atom(
                    &mut env.symtab,
                    &format!("\"{path}\""),
                ));
            }
            Err(_) => continue, // already exists, retry
        }
    }
    Err(YacasError::FileNotFound)
}

/// Secure — `Secure(body)` (Macro|Fixed): sets env.secure=true while the body
/// is evaluated (restored on exit, like upstream LispSecureFrame: set on
/// entry, restore the previous value on exit). CheckSecure commands called
/// inside the body (SystemCall/ToFile/FromFile/Load etc.) then raise
/// SecurityBreach. The openmath `Secure(Eval(...))` pattern depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispSecure.
pub fn cmd_secure(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 1 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let prev = env.secure;
    env.secure = true;
    // Macro: the body is not evaluated by the dispatcher; eval here (the secure
    // flag is restored manually so it recovers even on error)
    let result = eval(env, arg(inner, n - 1)?);
    env.secure = prev;
    result
}

// ==================== Input stream family FromString/Read/ReadToken ====================
/// FromString — `FromString("str")body` (Macro|Fixed + bodied): evaluates
/// argument 1 to a string, pushes a new input stream (the tokenizer holds the
/// string + cursor), evaluates the body (its Read/ReadToken read from and
/// advance that string), then pops the stack to restore. Returns the body's
/// result. The yacasinit REPL `FromString(input)Read()` depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFromString.
pub fn cmd_from_string(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let text = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let mut tok = crate::tokenizer::Tokenizer::new(&text);
    tok.xml = env.xml_tokenizer.get();
    env.input_stack.borrow_mut().push(Some(tok));
    // Input state = "String" (like upstream LispFromString SetTo/RestoreFrom)
    let old_file = env.input_file.borrow().clone();
    *env.input_file.borrow_mut() = "String".to_string();
    let body_result = eval(env, arg(inner, n - 1)?);
    *env.input_file.borrow_mut() = old_file;
    env.input_stack.borrow_mut().pop();
    body_result
}

/// FromFile — `FromFile("name")body` (Macro|Fixed + bodied): evaluates the
/// argument to a file name, looks it up via input_directories (bare names:
/// CWD first, like the LispLocalFile read path); open failure -> FileNotFound.
/// Pushes a file-content input stream, evaluates the body (Read/ReadToken read
/// from the file), pops to restore. Returns the body's result. CheckSecure.
/// The sql toolchain's FromFile(...)Read() in io.rep depends on this.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispFromFile.
pub fn cmd_from_file(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    let n = arity_of(inner);
    if n < 2 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    if env.secure {
        return Err(YacasError::SecurityBreach);
    }
    let name_node = eval(env, arg(inner, 0)?)?;
    let s = name_node.atom_string().ok_or(YacasError::InvalidArg)?;
    let fname = crate::standard::internal_unstringify(s)
        .unwrap_or(s)
        .to_string();
    let path = crate::standard::internal_find_file(env, &fname).ok_or(YacasError::FileNotFound)?;
    let text = std::fs::read_to_string(&path).map_err(|_| YacasError::FileNotFound)?;
    let mut tok = crate::tokenizer::Tokenizer::new(&text);
    tok.xml = env.xml_tokenizer.get();
    env.input_stack.borrow_mut().push(Some(tok));
    // Input state = file name (like upstream LispFromFile SetTo/RestoreFrom)
    let old_file = env.input_file.borrow().clone();
    *env.input_file.borrow_mut() = fname.clone();
    let body_result = eval(env, arg(inner, n - 1)?);
    *env.input_file.borrow_mut() = old_file;
    env.input_stack.borrow_mut().pop();
    body_result
}
/// Read (Function|Fixed, arity 0): reads one expression from the current
/// input stream (like upstream InfixParser::Parse: up to `;` or EOF; the
/// input string must end with `;` or "Error parsing expression" is raised).
/// Errors when no input stream is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispRead.
pub fn cmd_read(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    // Take the tokenizer off the stack top (releasing the borrow), parse one
    // expression, then put it back (cursor advanced).
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic(
                "Read: no active input stream".to_string(),
            ))?
            .take()
            .ok_or(YacasError::Generic(
                "Read: input tokenizer missing".to_string(),
            ))?
    };
    let expr = crate::parser::parse_one(env, &mut tok);
    // Put back (even on parse error; the cursor stays at the failure point)
    env.input_stack
        .borrow_mut()
        .last_mut()
        .expect("input")
        .replace(tok);
    match expr {
        Ok(Some(e)) => {
            // EndOfFile atom -> return the EndOfFile symbol (upstream Read at end of stream)
            if e.atom_string()
                .map(|s| s.as_ref() == "EndOfFile")
                .unwrap_or(false)
            {
                Ok(LispObject::atom(env.symtab.look_up("EndOfFile")))
            } else {
                // Parse only, do not evaluate: FromString("x;")Read() returns x unevaluated.
                Ok(e)
            }
        }
        Ok(None) => Err(YacasError::Generic("Read: parse error".to_string())),
        Err(_) => Err(YacasError::Generic("Error parsing expression".to_string())),
    }
}

/// ReadToken (Function|Fixed, arity 0): reads one token from the current
/// input stream (verbatim atom; end of stream -> EndOfFile). Errors when no
/// input stream is active.
/// See upstream: cyacas/libyacas/src/mathcommands.cpp LispReadToken.
pub fn cmd_read_token(
    env: &mut Environment,
    inner: &Rc<LispObject>,
) -> Result<Rc<LispObject>, YacasError> {
    if arity_of(inner) != 0 {
        return Err(YacasError::WrongNumberOfArgs);
    }
    let mut tok = {
        let mut stack = env.input_stack.borrow_mut();
        stack
            .last_mut()
            .ok_or(YacasError::Generic(
                "ReadToken: no active input stream".to_string(),
            ))?
            .take()
            .ok_or(YacasError::Generic(
                "ReadToken: input tokenizer missing".to_string(),
            ))?
    };
    let token = tok.next_token();
    env.input_stack
        .borrow_mut()
        .last_mut()
        .expect("input")
        .replace(tok);
    let token = token.unwrap_or_default();
    if token.is_empty() {
        Ok(LispObject::atom(env.symtab.look_up("EndOfFile")))
    } else {
        // Build the atom verbatim (like upstream LispAtom::New(*result); numbers/strings created by content)
        Ok(crate::value::atom_or_number(&mut env.symtab, &token))
    }
}
