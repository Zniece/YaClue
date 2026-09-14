use std::rc::Rc;

use yacas_rs::env::Environment;
use yacas_rs::value::LispObject;

use super::normalization_representation::RepresentationId;
use super::object_state::{
    ExpressionView, MathematicalIdentity, MathematicalObject, ObjectRevision, SemanticState,
};

/// A bounded, computation-local AST selection. It is deliberately not stored
/// on `MathematicalObject`, so temporary expansions cannot leak into the
/// object's stable representation set.
#[derive(Clone)]
pub struct OperationSessionAst {
    pub identity: MathematicalIdentity,
    pub revision: ObjectRevision,
    pub(super) expression: Rc<LispObject>,
}

impl OperationSessionAst {
    pub fn view<'a>(&'a self, env: &'a Environment) -> ExpressionView<'a> {
        ExpressionView::new(env, &self.expression)
    }

    pub(crate) fn materialize(&self, semantics: SemanticState) -> MathematicalObject {
        let mut object =
            MathematicalObject::new(self.identity.0, self.expression.clone(), semantics);
        object.revision = self.revision;
        object
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCacheKey {
    pub operation: String,
    pub assumptions: Vec<String>,
    pub precision: Option<u32>,
    pub revision: ObjectRevision,
    pub representation: RepresentationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationCacheBudget {
    pub max_entries: usize,
    pub max_total_bytes: usize,
    pub max_entry_bytes: usize,
}

impl Default for OperationCacheBudget {
    fn default() -> Self {
        Self {
            max_entries: 8,
            max_total_bytes: 64 * 1024,
            max_entry_bytes: 16 * 1024,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CachedOperationResult {
    pub expression: Rc<LispObject>,
    pub source: String,
    pub tex: String,
}

#[derive(Clone)]
pub(super) struct OperationCacheEntry {
    pub(super) key: OperationCacheKey,
    pub(super) result: CachedOperationResult,
    pub(super) bytes: usize,
}

#[derive(Clone, Default)]
pub(super) struct OperationCache {
    pub(super) entries: Vec<OperationCacheEntry>,
    pub(super) total_bytes: usize,
}
