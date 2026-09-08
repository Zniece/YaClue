//! Substitution behaviours; the recursive driver lives in
//! `standard::internal_substitute`. See upstream
//! `cyacas/libyacas/src/substitute.cpp`.
//!
//! Behaviour interface: `matches` returns `Some(replacement)` on a hit,
//! `None` to leave the element untouched.
//! - `SubstBehaviourImpl`: equality with the target → replace by a copy.
//! - `BackQuoteBehaviour`: preserves `` `(...) ``; `@a` evaluates the atom;
//!   `@f(args)` evaluates the head, rebuilds the call, and recurses.
//! - `LocalSymbolBehaviour`: renames matched symbols to unique fresh names.

use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::value::{copy_node, LispObject, ObjectKind};

pub trait SubstBehaviour {
    fn matches(
        &mut self,
        env: &mut Environment,
        element: &Rc<LispObject>,
    ) -> Result<Option<Rc<LispObject>>, YacasError>;
}

/// Plain substitution: element equal to the target → replace by a copy.
pub struct SubstBehaviourImpl {
    to_match: Rc<LispObject>,
    to_replace_with: Rc<LispObject>,
}

impl SubstBehaviourImpl {
    pub fn new(to_match: &Rc<LispObject>, to_replace_with: &Rc<LispObject>) -> Self {
        SubstBehaviourImpl {
            to_match: to_match.clone(),
            to_replace_with: to_replace_with.clone(),
        }
    }
}

impl SubstBehaviour for SubstBehaviourImpl {
    fn matches(
        &mut self,
        env: &mut Environment,
        element: &Rc<LispObject>,
    ) -> Result<Option<Rc<LispObject>>, YacasError> {
        if crate::standard::internal_equals(env, element, &self.to_match) {
            return Ok(Some(copy_node(&self.to_replace_with)));
        }
        Ok(None)
    }
}

/// Backquote behaviour.
pub struct BackQuoteBehaviour {
    /// Positional pairs for macro expansion (formal name → argument). Inside
    /// a macro body, `@y` expands to the *argument*, not to a global/local
    /// variable of the same name.
    pairs: Option<Vec<(Rc<str>, Rc<LispObject>)>>,
}

impl BackQuoteBehaviour {
    pub fn new(_env: &mut Environment) -> Self {
        BackQuoteBehaviour { pairs: None }
    }

    /// Positional substitution used by macro expansion.
    pub fn new_with_pairs(_env: &mut Environment, pairs: Vec<(Rc<str>, Rc<LispObject>)>) -> Self {
        BackQuoteBehaviour { pairs: Some(pairs) }
    }
}

impl SubstBehaviour for BackQuoteBehaviour {
    fn matches(
        &mut self,
        env: &mut Environment,
        element: &Rc<LispObject>,
    ) -> Result<Option<Rc<LispObject>>, YacasError> {
        let inner = match element.sublist() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let head = inner;
        let head_str = match head.atom_string() {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        // `(...) is kept verbatim (recursion must not re-enter it)
        if head_str.as_ref() == "`" {
            return Ok(Some(element.clone()));
        }
        if head_str.as_ref() != "@" {
            return Ok(None);
        }
        let arg = match head.next.as_ref() {
            Some(a) => a,
            None => return Ok(None),
        };
        if arg.atom_string().is_some() {
            // @a: if `pairs` contains this formal name, substitute the
            // argument copy directly; otherwise evaluate and replace.
            if let Some(pairs) = &self.pairs {
                let name = arg.atom_string().unwrap();
                if let Some((_, node)) = pairs.iter().find(|(n, _)| *n == *name) {
                    return Ok(Some(copy_node(node)));
                }
            }
            return Ok(Some(crate::evaluator::eval(env, arg)?));
        }
        // @f(args): evaluate the head, keep the args, rebuild
        // (newhead args...), and substitute recursively.
        let f = arg.sublist().ok_or(YacasError::InvalidArg)?;
        let new_head = crate::evaluator::eval(env, f)?;
        let mut kinds: Vec<ObjectKind> = Vec::new();
        kinds.push(
            crate::value::spine_kinds(&new_head)
                .next()
                .expect("new head kind"),
        );
        let mut cur = f.next.as_ref();
        while let Some(n) = cur {
            kinds.push(crate::value::spine_kinds(n).next().expect("arg kind"));
            cur = n.next.as_ref();
        }
        let inner2 = crate::value::build_list(kinds).expect("rebuild inner");
        let node2 = Rc::new(LispObject {
            next: None,
            kind: ObjectKind::Sublist(inner2),
        });
        let mut behaviour = BackQuoteBehaviour::new(env);
        let result = crate::standard::internal_substitute(env, &node2, &mut behaviour)?;
        Ok(Some(result))
    }
}

/// Inline rule arguments into a rule body: when a rule matches, pattern
/// variables in the body are substituted with the argument nodes *before*
/// evaluation, while predicates keep using frame-local bindings. Frame-local
/// reading alone is not sufficient: `eval(atom)` short-circuits to the raw
/// literal and drags in the list head, breaking `FlatCopy`/`Length` on list
/// arguments.
pub struct InlineArgBehaviour {
    pairs: Vec<(Rc<str>, Rc<LispObject>)>,
}

impl InlineArgBehaviour {
    pub fn new(pairs: Vec<(Rc<str>, Rc<LispObject>)>) -> Self {
        InlineArgBehaviour { pairs }
    }
}

impl SubstBehaviour for InlineArgBehaviour {
    fn matches(
        &mut self,
        _env: &mut Environment,
        element: &Rc<LispObject>,
    ) -> Result<Option<Rc<LispObject>>, YacasError> {
        let Some(name) = element.atom_string() else {
            return Ok(None);
        };
        if let Some((_, value)) = self.pairs.iter().find(|(n, _)| Rc::ptr_eq(n, name)) {
            return Ok(Some(copy_node(value)));
        }
        Ok(None)
    }
}

/// Local-symbol renaming: matched names become unique fresh names.
pub struct LocalSymbolBehaviour {
    original_names: Vec<Rc<str>>,
    new_names: Vec<Rc<str>>,
}

impl LocalSymbolBehaviour {
    pub fn new(env: &mut Environment, names: &[Rc<str>]) -> Self {
        let original_names = names.to_vec();
        // Yacas gives every name in one LocalSymbols invocation the same
        // generation suffix: LocalSymbols(a,b) produces $aN and $bN.  Some
        // standard scripts (notably UniqueConstant) consume this public
        // spelling, so it is part of the language contract.
        let id = env.gen_unique_id();
        let mut new_names = Vec::with_capacity(names.len());
        for name in names {
            let new_name = env.symtab.look_up(&format!("${name}{id}"));
            new_names.push(new_name);
        }
        LocalSymbolBehaviour {
            original_names,
            new_names,
        }
    }
}

impl SubstBehaviour for LocalSymbolBehaviour {
    fn matches(
        &mut self,
        _env: &mut Environment,
        element: &Rc<LispObject>,
    ) -> Result<Option<Rc<LispObject>>, YacasError> {
        let name = match element.atom_string() {
            Some(s) => s,
            None => return Ok(None),
        };
        for (i, orig) in self.original_names.iter().enumerate() {
            if Rc::ptr_eq(orig, name) {
                // Hit: build an atom node for the new name, preserving the
                // original node's `next` pointer.
                let mut node = LispObject::atom(self.new_names[i].clone());
                if let Some(m) = Rc::get_mut(&mut node) {
                    m.next = element.next.clone();
                }
                return Ok(Some(node));
            }
        }
        Ok(None)
    }
}
