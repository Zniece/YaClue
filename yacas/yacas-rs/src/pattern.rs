//! Pattern matching. See upstream `cyacas/libyacas/src/patterns.cpp`
//! (`YacasPatternPredicateBase`, `MatchAtom`, `MatchNumber`,
//! `MatchVariable`, `MatchSubList`).
//!
//! Matching is two-phase: first match every argument (a pattern-variable
//! slot stores on first sight; a bound slot must compare equal), then bind
//! tentatively in a scratch frame and evaluate the post-predicates; on
//! failure everything rolls back, on success the variables are bound once
//! more for the rule body.
//!
//! Pattern shapes: number → `MatchNumber`, atom → `MatchAtom`,
//! `(_ var)` → `MatchVariable`, `(_ var pred)` → `MatchVariable` + predicate,
//! other sublists → `MatchSubList` recursion.

use std::rc::Rc;

use crate::env::Environment;
use crate::errors::YacasError;
use crate::standard::{internal_equals, is_false, is_true};
use crate::value::ObjectKind;
use crate::value::{copy_node, spine_refs, LispObject};

/// Single-argument matcher.
pub trait ParamMatcher {
    fn argument_matches(
        &self,
        env: &mut Environment,
        expression: &Rc<LispObject>,
        arguments: &mut [Option<Rc<LispObject>>],
    ) -> Result<bool, YacasError>;
}

/// Match a specific atom. Numbers never match atoms (even integral floats).
pub struct MatchAtom {
    string: Rc<str>,
}

impl MatchAtom {
    pub fn new(string: Rc<str>) -> Self {
        MatchAtom { string }
    }
}

impl ParamMatcher for MatchAtom {
    fn argument_matches(
        &self,
        _env: &mut Environment,
        expression: &Rc<LispObject>,
        _arguments: &mut [Option<Rc<LispObject>>],
    ) -> Result<bool, YacasError> {
        match &expression.kind {
            crate::value::ObjectKind::Number(_) => Ok(false),
            crate::value::ObjectKind::Atom(s) => Ok(Rc::ptr_eq(s, &self.string)),
            _ => Ok(false),
        }
    }
}

/// Match a specific number by value at environment precision. The pattern
/// side is stored as text.
pub struct MatchNumber {
    num_text: String,
}

impl MatchNumber {
    pub fn new(num_text: &str) -> Self {
        MatchNumber {
            num_text: num_text.to_string(),
        }
    }
}

impl ParamMatcher for MatchNumber {
    fn argument_matches(
        &self,
        env: &mut Environment,
        expression: &Rc<LispObject>,
        _arguments: &mut [Option<Rc<LispObject>>],
    ) -> Result<bool, YacasError> {
        let target = match &expression.kind {
            crate::value::ObjectKind::Number(n) => n.float_at(env.precision()),
            _ => return Ok(false),
        };
        let pat =
            crate::number::float::Float::from_decimal_with_prec(&self.num_text, env.precision())
                .ok_or(YacasError::InvalidArg)?;
        Ok(pat.equals(&target))
    }
}

/// A pattern variable: an empty slot stores and matches; a bound slot
/// requires equality (repeated variables must bind consistently).
pub struct MatchVariable {
    var_index: usize,
}

impl MatchVariable {
    pub fn new(var_index: usize) -> Self {
        MatchVariable { var_index }
    }
}

impl ParamMatcher for MatchVariable {
    fn argument_matches(
        &self,
        env: &mut Environment,
        expression: &Rc<LispObject>,
        arguments: &mut [Option<Rc<LispObject>>],
    ) -> Result<bool, YacasError> {
        if arguments[self.var_index].is_none() {
            arguments[self.var_index] = Some(expression.clone());
            return Ok(true);
        }
        let prev = arguments[self.var_index].clone().expect("slot is bound");
        Ok(internal_equals(env, expression, &prev))
    }
}

/// Element-wise sublist match: every matcher must consume a node in order
/// and the sublist must be exhausted exactly.
pub struct MatchSubList {
    matchers: Vec<Box<dyn ParamMatcher>>,
}

impl MatchSubList {
    pub fn new(matchers: Vec<Box<dyn ParamMatcher>>) -> Self {
        MatchSubList { matchers }
    }
}

impl ParamMatcher for MatchSubList {
    fn argument_matches(
        &self,
        env: &mut Environment,
        expression: &Rc<LispObject>,
        arguments: &mut [Option<Rc<LispObject>>],
    ) -> Result<bool, YacasError> {
        let inner = match expression.sublist() {
            Some(inner) => inner,
            None => return Ok(false),
        };
        let mut iter = spine_refs(inner);
        for m in &self.matchers {
            let node = match iter.next() {
                Some(n) => n,
                None => return Ok(false),
            };
            if !m.argument_matches(env, node, arguments)? {
                return Ok(false);
            }
        }
        Ok(iter.next().is_none())
    }
}

/// Pattern predicate engine: a matcher sequence, a variable table, and
/// post-predicates.
pub struct PatternPredicate {
    param_matchers: Vec<Box<dyn ParamMatcher>>,
    variables: Vec<Rc<str>>,
    predicates: Vec<Rc<LispObject>>,
}

impl PatternPredicate {
    /// Build from a full pattern expression; each argument becomes a matcher
    /// and the post-predicate is copied into the predicate list.
    pub fn new(
        env: &mut Environment,
        pattern: &Rc<LispObject>,
        post_predicate: &Rc<LispObject>,
    ) -> Result<Self, YacasError> {
        let mut pp = PatternPredicate {
            param_matchers: Vec::new(),
            variables: Vec::new(),
            predicates: Vec::new(),
        };
        let iter = spine_refs(pattern);
        for node in iter {
            let matcher = pp.make_param_matcher(env, node)?;
            pp.param_matchers.push(matcher);
        }
        pp.predicates.push(copy_node(post_predicate));
        Ok(pp)
    }

    /// Match: per-argument matching → tentative binding + predicates in a
    /// scratch frame → on success, bind once more for the rule body.
    pub fn matches(
        &self,
        env: &mut Environment,
        arguments: &[Rc<LispObject>],
    ) -> Result<bool, YacasError> {
        let mut slots: Vec<Option<Rc<LispObject>>> = vec![None; self.variables.len()];
        if self.param_matchers.len() != arguments.len() {
            return Ok(false);
        }
        for (m, arg) in self.param_matchers.iter().zip(arguments.iter()) {
            if !m.argument_matches(env, arg, &mut slots)? {
                return Ok(false);
            }
        }
        env.push_local_frame(false);
        let ok = {
            self.set_pattern_variables(env, &slots);
            self.check_predicates(env)
        };
        env.pop_local_frame()?;
        if !ok? {
            return Ok(false);
        }
        self.set_pattern_variables(env, &slots);
        Ok(true)
    }

    /// Build a matcher for one pattern argument. `(_ var [pred])` shapes
    /// become variables; the optional predicate is the node next to `var`
    /// (a sublist predicate is flattened into its content chain), with the
    /// variable atom appended. E.g. `expr_IsFreeOf(cc)` is
    /// `(_ expr (IsFreeOf cc))` → predicate `(IsFreeOf cc expr)`.
    fn make_param_matcher(
        &mut self,
        env: &mut Environment,
        pattern: &Rc<LispObject>,
    ) -> Result<Box<dyn ParamMatcher>, YacasError> {
        match &pattern.kind {
            crate::value::ObjectKind::Number(_) => {
                let text = pattern.number_string().expect("number text");
                Ok(Box::new(MatchNumber::new(&text)))
            }
            crate::value::ObjectKind::Atom(s) => Ok(Box::new(MatchAtom::new(s.clone()))),
            _ => {
                let sublist = pattern.sublist().ok_or(YacasError::InvalidArg)?;
                let num = crate::standard::internal_list_length(sublist);
                // Variable template `(_ var ...)`.
                if num > 1 {
                    let head = sublist;
                    if head.atom_string().map(|s| s.as_ref()) == Some("_") {
                        if let Some(second) = head.next.as_ref() {
                            if let Some(var) = second.atom_string() {
                                let index = self.look_up_var(var.clone());
                                if num > 2 {
                                    let pred_node =
                                        second.next.as_ref().ok_or(YacasError::InvalidArg)?;
                                    let mut parts: Vec<Rc<LispObject>> = Vec::new();
                                    match pred_node.sublist() {
                                        // Sublist predicate: flatten its content chain.
                                        Some(inner) => {
                                            for n in crate::value::spine_refs(inner) {
                                                parts.push(copy_node(n));
                                            }
                                        }
                                        None => parts.push(copy_node(pred_node)),
                                    }
                                    let var_sym = env.symtab.look_up(var.as_ref());
                                    parts.push(crate::value::LispObject::atom(var_sym));
                                    let kinds: Vec<ObjectKind> = parts
                                        .into_iter()
                                        .map(|n| {
                                            crate::value::spine_kinds(&n).next().expect("kind")
                                        })
                                        .collect();
                                    let inner =
                                        crate::value::build_list(kinds).expect("pred inner");
                                    let pred2 = Rc::new(LispObject {
                                        next: None,
                                        kind: crate::value::ObjectKind::Sublist(inner),
                                    });
                                    self.predicates.push(pred2);
                                }
                                return Ok(Box::new(MatchVariable::new(index)));
                            }
                        }
                    }
                }
                // Ordinary sublist: recurse element-wise.
                let mut matchers: Vec<Box<dyn ParamMatcher>> = Vec::new();
                for node in spine_refs(sublist) {
                    matchers.push(self.make_param_matcher(env, node)?);
                }
                Ok(Box::new(MatchSubList::new(matchers)))
            }
        }
    }

    /// Build from a variable-name list (the `Pattern'Create` shape): each
    /// entry becomes a matcher; the post-predicate goes into the predicate
    /// list. `vars` may be empty, yielding a zero-argument matcher.
    ///
    /// Entry shapes:
    /// - plain atom (no `_` suffix) → literal `MatchAtom` (rule left sides
    ///   may contain bare atoms like `Undefined` that are *not* variables);
    /// - atom with a `_` suffix (`a_IsNumber`) → variable + predicate;
    /// - number → `MatchNumber`;
    /// - `(_ var [pred])` template → variable (+ flattened predicate);
    /// - any other sublist (e.g. `(if ...)` templates) → recursive matcher.
    pub fn from_vars(
        env: &mut Environment,
        vars: Option<&Rc<LispObject>>,
        post_predicate: &Rc<LispObject>,
    ) -> Result<Self, YacasError> {
        let mut pp = PatternPredicate {
            param_matchers: Vec::new(),
            variables: Vec::new(),
            predicates: Vec::new(),
        };
        if let Some(vars) = vars {
            let mut cur: Option<&Rc<LispObject>> = Some(vars);
            while let Some(n) = cur {
                let matcher: Box<dyn ParamMatcher> = if let Some(s) = n.atom_string() {
                    // Atom with a `_` suffix → variable + predicate; without →
                    // literal atom match.
                    let (var_name, pred_name) = match s.rfind('_') {
                        Some(i) if i > 0 && i + 1 < s.len() => {
                            (Rc::from(&s[..i]), Rc::from(&s[i + 1..]))
                        }
                        _ => (s.clone(), Rc::<str>::default()),
                    };
                    if pred_name.is_empty() {
                        Box::new(MatchAtom::new(s.clone()))
                    } else {
                        let index = pp.look_up_var(var_name.clone());
                        let head_sym = env.symtab.look_up(&pred_name);
                        let var_atom =
                            crate::value::LispObject::atom(env.symtab.look_up(&var_name));
                        let inner = crate::value::build_list(vec![
                            ObjectKind::Atom(head_sym),
                            crate::value::spine_kinds(&var_atom)
                                .next()
                                .expect("var kind"),
                        ])
                        .expect("pred inner is non-empty");
                        let pred_node = Rc::new(LispObject {
                            next: None,
                            kind: ObjectKind::Sublist(inner),
                        });
                        pp.predicates.push(pred_node);
                        Box::new(MatchVariable::new(index))
                    }
                } else if let Some(num_text) = n.number_string() {
                    Box::new(MatchNumber::new(&num_text))
                } else if let Some(sub) = n.sublist() {
                    if sub.atom_string().map(|s| s.as_ref()) == Some("_") {
                        match sub.next.as_ref().and_then(|v| v.atom_string()) {
                            Some(var) => {
                                let index = pp.look_up_var(var.clone());
                                // `(_ var [pred...])`: optional predicate next to
                                // `var`, flattened if a sublist, variable appended.
                                let num = crate::standard::internal_list_length(sub);
                                if num > 2 {
                                    let pred_node = sub
                                        .next
                                        .as_ref()
                                        .and_then(|v| v.next.as_ref())
                                        .ok_or(YacasError::InvalidArg)?;
                                    let mut pred: Vec<Rc<LispObject>> = Vec::new();
                                    match pred_node.sublist() {
                                        Some(inner) => {
                                            for n in crate::value::spine_refs(inner) {
                                                pred.push(copy_node(n));
                                            }
                                        }
                                        None => pred.push(copy_node(pred_node)),
                                    }
                                    let var_sym = env.symtab.look_up(var.as_ref());
                                    pred.push(crate::value::LispObject::atom(var_sym));
                                    let inner = crate::value::build_list(
                                        pred.into_iter()
                                            .map(|n| {
                                                crate::value::spine_kinds(&n).next().expect("kind")
                                            })
                                            .collect(),
                                    )
                                    .expect("pred inner");
                                    let pred2 = Rc::new(LispObject {
                                        next: None,
                                        kind: ObjectKind::Sublist(inner),
                                    });
                                    pp.predicates.push(pred2);
                                }
                                Box::new(MatchVariable::new(index))
                            }
                            // `(_ (sub...) ...)`: the variable slot itself is a
                            // sublist — delegate to the recursive matcher.
                            None => pp.make_param_matcher(env, n)?,
                        }
                    } else {
                        pp.make_param_matcher(env, n)?
                    }
                } else {
                    return Err(YacasError::InvalidArg);
                };
                pp.param_matchers.push(matcher);
                cur = n.next.as_ref();
            }
        }
        pp.predicates.push(copy_node(post_predicate));
        Ok(pp)
    }

    fn look_up_var(&mut self, name: Rc<str>) -> usize {
        if let Some(i) = self.variables.iter().position(|v| Rc::ptr_eq(v, &name)) {
            return i;
        }
        self.variables.push(name);
        self.variables.len() - 1
    }

    fn set_pattern_variables(&self, env: &mut Environment, slots: &[Option<Rc<LispObject>>]) {
        for (i, name) in self.variables.iter().enumerate() {
            env.new_local(name.clone(), slots[i].clone());
        }
    }

    fn check_predicates(&self, env: &mut Environment) -> Result<bool, YacasError> {
        for pred in &self.predicates {
            let p = crate::evaluator::eval(env, pred)?;
            if std::env::var_os("YACAS_TRACE_LOAD").is_some() {
                eprintln!(
                    "[PRED] {} => {}",
                    crate::printer::infix_print(env, pred),
                    crate::printer::infix_print(env, &p)
                );
            }
            if is_false(env, &p) {
                return Ok(false);
            }
            if !is_true(env, &p) {
                // A non-boolean predicate result (e.g. the predicate function
                // is undefined and stays unevaluated) means "this rule does
                // not match" — matching upstream. It must NOT abort the
                // rule chain with an error: later rules with lower
                // precedence still apply.
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Pattern generic object: wraps a `PatternPredicate` for transport through
/// `Pattern'Create`/`MacroRulePattern`.
pub struct PatternClass {
    pub pattern: PatternPredicate,
}

impl crate::value::GenericClass for PatternClass {
    fn type_name(&self) -> &'static str {
        "\"Pattern\""
    }

    fn matches_pattern(
        &self,
        env: &mut Environment,
        arguments: &[Rc<LispObject>],
    ) -> Result<Option<bool>, YacasError> {
        Ok(Some(self.pattern.matches(env, arguments)?))
    }
}
