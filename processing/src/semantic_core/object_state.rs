use std::rc::Rc;

use serde::Serialize;
use yacas_rs::env::Environment;
use yacas_rs::value::{spine_refs, LispObject, ObjectKind};

mod model;
pub(crate) use model::parse_engine_expression;
pub use model::*;

/// Stable only for the lifetime of one computation. It is intentionally not
/// a pointer or a symbol spelling so future AST rewrites can preserve object
/// identity without exposing engine storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// Monotonically increasing version of one mathematical object. A path is
/// meaningful only together with this revision; a later rewrite must never
/// reinterpret an old event's focus against the new AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRevision(pub u64);

/// Identity of the mathematical value, independent of which equivalent AST
/// is currently preferred for display or for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MathematicalIdentity(pub ObjectId);

/// A path into the current AST. Paths are scoped to one computation and are
/// invalidated by a rewrite that changes their ancestor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ExpressionPath(Vec<usize>);

impl ExpressionPath {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn argument(&self, index: usize) -> Self {
        let mut path = self.0.clone();
        path.push(index);
        Self(path)
    }

    pub fn segments(&self) -> &[usize] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Atom,
    Number,
    Application,
    List,
    Generic,
}

/// Storage-independent, read-only AST view. Creating a view only borrows the
/// existing engine node; it never allocates a mirror tree or reparses text.
pub struct ExpressionView<'a> {
    env: &'a Environment,
    node: &'a Rc<LispObject>,
}

impl<'a> ExpressionView<'a> {
    pub fn new(env: &'a Environment, node: &'a Rc<LispObject>) -> Self {
        Self { env, node }
    }

    pub(crate) fn raw_expression(&self) -> Rc<LispObject> {
        self.node.clone()
    }

    pub fn kind(&self) -> NodeKind {
        match &self.node.kind {
            ObjectKind::Atom(_) => NodeKind::Atom,
            ObjectKind::Number(_) => NodeKind::Number,
            ObjectKind::Generic(_) => NodeKind::Generic,
            ObjectKind::Sublist(first) => {
                if first.atom_string().is_some() {
                    NodeKind::Application
                } else {
                    NodeKind::List
                }
            }
        }
    }

    pub fn head(&self) -> Option<&str> {
        match &self.node.kind {
            ObjectKind::Sublist(first) => first.atom_string().map(|head| head.as_ref()),
            _ => None,
        }
    }

    pub fn atom(&self) -> Option<&str> {
        self.node.atom_string().map(|atom| atom.as_ref())
    }

    pub fn number(&self) -> Option<String> {
        self.node.number_string()
    }

    pub fn arguments(&self) -> Vec<ExpressionView<'a>> {
        let ObjectKind::Sublist(first) = &self.node.kind else {
            return Vec::new();
        };
        spine_refs(first)
            .skip(1)
            .map(|node| ExpressionView {
                env: self.env,
                node,
            })
            .collect()
    }

    /// Resolve a semantic AST path without allocating or reparsing a mirror
    /// expression. Path segments address application arguments (the head is
    /// deliberately excluded).
    pub fn at_path(&self, path: &ExpressionPath) -> Option<ExpressionView<'a>> {
        let mut node = self.node;
        for index in path.segments() {
            let ObjectKind::Sublist(first) = &node.kind else {
                return None;
            };
            node = spine_refs(first).skip(1).nth(*index)?;
        }
        Some(ExpressionView {
            env: self.env,
            node,
        })
    }

    /// Compatibility boundary for an engine consumer that still needs source
    /// text. Semantic-core internals should pass views, not this string.
    pub fn print_source(&self) -> String {
        yacas_rs::printer::infix_print(self.env, self.node)
    }
}
