use std::rc::Rc;
use yacas_rs::value::LispObject;

use super::object_state::SemanticOverlay;
use super::operator_signature::OperatorId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepresentationId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepresentationPreference {
    Principal,
    Operation(OperatorId),
    Named(String),
}

#[derive(Clone)]
pub(super) struct StableRepresentation {
    pub(super) id: RepresentationId,
    pub(super) preference: RepresentationPreference,
    pub(super) expression: Rc<LispObject>,
    pub(super) overlay: SemanticOverlay,
    pub(super) normalization: Option<NormalizationMetadata>,
}

#[derive(Clone)]
pub(super) struct RepresentationSet {
    pub(super) candidates: Vec<StableRepresentation>,
    pub(super) active: RepresentationId,
    pub(super) next_id: u64,
}

pub(crate) const MAX_STABLE_REPRESENTATIONS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NormalizationLevel {
    Structural,
    Domain,
    Conditional,
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationMode {
    Safe,
    Operation(OperatorId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationMetadata {
    pub level: NormalizationLevel,
    pub assumptions: Vec<crate::protocol::Condition>,
    pub mode: NormalizationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizationState {
    pub revision: super::ObjectRevision,
    pub metadata: NormalizationMetadata,
}
