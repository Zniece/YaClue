//! Small cross-domain condition and outcome protocol.
//!
//! Domain-specific result enums remain authoritative. These types provide a
//! stable facade for composition and product presentation without replacing
//! the richer domain structures.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::engine::EngineError;
use crate::semantic::Exactness;

pub const MAX_CONDITIONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "predicate", rename_all = "snake_case")]
pub enum Condition {
    RealPartPositive { expression: String },
    Positive { expression: String },
    Negative { expression: String },
    NonZero { expression: String },
    Real { expression: String },
    Integer { expression: String },
    Unknown { description: String },
}

impl Condition {
    fn expression(&self) -> Option<&str> {
        match self {
            Self::RealPartPositive { expression }
            | Self::Positive { expression }
            | Self::Negative { expression }
            | Self::NonZero { expression }
            | Self::Real { expression }
            | Self::Integer { expression } => Some(expression),
            Self::Unknown { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionConsistency {
    Consistent,
    Contradictory,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConditionSet {
    conditions: Vec<Condition>,
    pub consistency: ConditionConsistency,
}

impl ConditionSet {
    pub fn empty() -> Self {
        Self {
            conditions: Vec::new(),
            consistency: ConditionConsistency::Consistent,
        }
    }

    pub fn new(conditions: impl IntoIterator<Item = Condition>) -> Result<Self, EngineError> {
        let mut unique: BTreeSet<_> = conditions.into_iter().collect();
        if unique.len() > MAX_CONDITIONS {
            return Err(EngineError::InvalidInput(format!(
                "条件数量超过上限 {MAX_CONDITIONS}"
            )));
        }

        // Only normalize implications among explicitly registered predicates.
        // No expression simplification or CAS query occurs here.
        let mut facts: BTreeMap<String, (bool, bool)> = BTreeMap::new();
        for condition in &unique {
            if let Some(expression) = condition.expression() {
                let entry = facts.entry(expression.into()).or_default();
                match condition {
                    Condition::Positive { .. } => entry.0 = true,
                    Condition::Negative { .. } => entry.1 = true,
                    _ => {}
                }
            }
        }
        for (expression, (positive, negative)) in &facts {
            if *positive || *negative {
                unique.remove(&Condition::Real {
                    expression: expression.clone(),
                });
                unique.remove(&Condition::NonZero {
                    expression: expression.clone(),
                });
            }
        }
        let integers = unique
            .iter()
            .filter_map(|condition| match condition {
                Condition::Integer { expression } => Some(expression.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for expression in integers {
            unique.remove(&Condition::Real { expression });
        }
        let consistency = if facts
            .values()
            .any(|(positive, negative)| *positive && *negative)
        {
            ConditionConsistency::Contradictory
        } else if unique
            .iter()
            .any(|condition| matches!(condition, Condition::Unknown { .. }))
        {
            ConditionConsistency::Unknown
        } else {
            ConditionConsistency::Consistent
        };
        Ok(Self {
            conditions: unique.into_iter().collect(),
            consistency,
        })
    }

    pub fn conditions(&self) -> &[Condition] {
        &self.conditions
    }

    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportState {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionState {
    Solved,
    Unresolved,
    NoResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultCompleteness {
    Complete,
    Representative,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Conditionality {
    Unconditional,
    Conditional,
    Insufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeReason {
    ConditionInsufficient,
    AlgorithmUncovered,
    MathematicalAbsence,
    Divergent,
    UnsupportedOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResultMetadata {
    pub support: SupportState,
    pub resolution: ResolutionState,
    pub exactness: Exactness,
    pub conditionality: Conditionality,
    pub completeness: ResultCompleteness,
    pub conditions: ConditionSet,
    pub reason: Option<OutcomeReason>,
}

impl ResultMetadata {
    pub fn solved(exactness: Exactness, conditions: ConditionSet) -> Self {
        let conditionality = if conditions.is_empty() {
            Conditionality::Unconditional
        } else {
            Conditionality::Conditional
        };
        Self {
            support: SupportState::Supported,
            resolution: ResolutionState::Solved,
            exactness,
            conditionality,
            completeness: ResultCompleteness::Complete,
            conditions,
            reason: None,
        }
    }

    pub fn unresolved(exactness: Exactness, reason: OutcomeReason) -> Self {
        Self {
            support: if reason == OutcomeReason::UnsupportedOperation {
                SupportState::Unsupported
            } else {
                SupportState::Supported
            },
            resolution: ResolutionState::Unresolved,
            exactness,
            conditionality: if reason == OutcomeReason::ConditionInsufficient {
                Conditionality::Insufficient
            } else {
                Conditionality::Unconditional
            },
            completeness: ResultCompleteness::Unknown,
            conditions: ConditionSet::empty(),
            reason: Some(reason),
        }
    }

    pub fn no_result(exactness: Exactness, reason: OutcomeReason) -> Self {
        Self {
            support: SupportState::Supported,
            resolution: ResolutionState::NoResult,
            exactness,
            conditionality: Conditionality::Unconditional,
            completeness: ResultCompleteness::Complete,
            conditions: ConditionSet::empty(),
            reason: Some(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression(value: &str) -> String {
        value.into()
    }

    #[test]
    fn normalizes_only_registered_predicate_implications() {
        let set = ConditionSet::new([
            Condition::Real {
                expression: expression("a"),
            },
            Condition::NonZero {
                expression: expression("a"),
            },
            Condition::Positive {
                expression: expression("a"),
            },
        ])
        .unwrap();
        assert_eq!(
            set.conditions(),
            &[Condition::Positive {
                expression: expression("a")
            }]
        );
        assert_eq!(set.consistency, ConditionConsistency::Consistent);
    }

    #[test]
    fn contradictory_and_unknown_conditions_stay_explicit() {
        let contradiction = ConditionSet::new([
            Condition::Positive {
                expression: expression("a"),
            },
            Condition::Negative {
                expression: expression("a"),
            },
        ])
        .unwrap();
        assert_eq!(
            contradiction.consistency,
            ConditionConsistency::Contradictory
        );

        let unknown = ConditionSet::new([Condition::Unknown {
            description: "a 或 b".into(),
        }])
        .unwrap();
        assert_eq!(unknown.consistency, ConditionConsistency::Unknown);
    }

    #[test]
    fn result_reasons_distinguish_failure_classes() {
        let condition =
            ResultMetadata::unresolved(Exactness::Symbolic, OutcomeReason::ConditionInsufficient);
        let algorithm =
            ResultMetadata::unresolved(Exactness::Unknown, OutcomeReason::AlgorithmUncovered);
        let absence =
            ResultMetadata::no_result(Exactness::Exact, OutcomeReason::MathematicalAbsence);
        assert_eq!(condition.conditionality, Conditionality::Insufficient);
        assert_eq!(algorithm.resolution, ResolutionState::Unresolved);
        assert_eq!(absence.resolution, ResolutionState::NoResult);
    }

    #[test]
    fn condition_count_is_bounded() {
        let conditions = (0..=MAX_CONDITIONS).map(|index| Condition::Integer {
            expression: format!("n{index}"),
        });
        assert!(ConditionSet::new(conditions).is_err());
    }
}
