//! Shared protocol for mathematical objects defined by a finite plan of
//! existing primitive operations.

use serde::Serialize;

use crate::protocol::ConditionSet;
use crate::steps::Step;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinedObjectKind {
    ImproperIntegral,
    CauchyPrincipalValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveOperation {
    FiniteIntegral,
    OneSidedLimit,
    SymmetricLimit,
    Assemble,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    Completed,
    Divergent,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectComponent {
    pub branch: usize,
    pub operation: PrimitiveOperation,
    pub input: String,
    pub value: String,
    pub status: ComponentStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinedObjectStatus {
    Converged,
    Divergent,
    Unresolved,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefinedObjectResult {
    pub kind: DefinedObjectKind,
    pub status: DefinedObjectStatus,
    pub source: String,
    pub value: String,
    pub tex: String,
    pub conditions: ConditionSet,
    pub components: Vec<ObjectComponent>,
    pub steps: Vec<Step>,
}
