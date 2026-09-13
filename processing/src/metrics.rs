//! Low-overhead, monotonic counters for semantic-pipeline baselines.
//!
//! These counters describe work rather than wall-clock policy. Consumers take
//! two snapshots around a single-threaded scenario and subtract them; normal
//! execution never resets shared process state.

use std::ops::Sub;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static ENGINE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static OBJECT_TRANSITIONS: AtomicU64 = AtomicU64::new(0);
static OBJECT_CLONES: AtomicU64 = AtomicU64::new(0);
static SESSION_AST_HANDLES: AtomicU64 = AtomicU64::new(0);
static RULE_PRESENTATIONS: AtomicU64 = AtomicU64::new(0);
static LEGACY_TRACE_ADAPTATIONS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionMetrics {
    pub parse_calls: u64,
    pub engine_requests: u64,
    pub object_transitions: u64,
    pub object_clones: u64,
    pub session_ast_handles: u64,
    pub rule_presentations: u64,
    pub legacy_trace_adaptations: u64,
}

impl ExecutionMetrics {
    pub fn snapshot() -> Self {
        Self {
            parse_calls: yacas_rs::parser::parse_call_count(),
            engine_requests: ENGINE_REQUESTS.load(Ordering::Relaxed),
            object_transitions: OBJECT_TRANSITIONS.load(Ordering::Relaxed),
            object_clones: OBJECT_CLONES.load(Ordering::Relaxed),
            session_ast_handles: SESSION_AST_HANDLES.load(Ordering::Relaxed),
            rule_presentations: RULE_PRESENTATIONS.load(Ordering::Relaxed),
            legacy_trace_adaptations: LEGACY_TRACE_ADAPTATIONS.load(Ordering::Relaxed),
        }
    }
}

impl Sub for ExecutionMetrics {
    type Output = Self;

    fn sub(self, earlier: Self) -> Self::Output {
        Self {
            parse_calls: self.parse_calls.saturating_sub(earlier.parse_calls),
            engine_requests: self.engine_requests.saturating_sub(earlier.engine_requests),
            object_transitions: self
                .object_transitions
                .saturating_sub(earlier.object_transitions),
            object_clones: self.object_clones.saturating_sub(earlier.object_clones),
            session_ast_handles: self
                .session_ast_handles
                .saturating_sub(earlier.session_ast_handles),
            rule_presentations: self
                .rule_presentations
                .saturating_sub(earlier.rule_presentations),
            legacy_trace_adaptations: self
                .legacy_trace_adaptations
                .saturating_sub(earlier.legacy_trace_adaptations),
        }
    }
}

#[derive(Debug)]
pub struct Measured<T> {
    pub value: T,
    pub metrics: ExecutionMetrics,
    pub elapsed: Duration,
}

pub fn measure<T>(operation: impl FnOnce() -> T) -> Measured<T> {
    let before = ExecutionMetrics::snapshot();
    let started = Instant::now();
    let value = operation();
    Measured {
        value,
        metrics: ExecutionMetrics::snapshot() - before,
        elapsed: started.elapsed(),
    }
}

pub(crate) fn record_engine_request() {
    ENGINE_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_object_transition() {
    OBJECT_TRANSITIONS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_object_clone() {
    OBJECT_CLONES.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_session_ast_handle() {
    SESSION_AST_HANDLES.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_rule_presentation() {
    RULE_PRESENTATIONS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_legacy_trace_adaptations(count: usize) {
    LEGACY_TRACE_ADAPTATIONS.fetch_add(count as u64, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::execute_steps;
    use crate::engine::RustEngine;
    use crate::steps::StepVerbosity;

    #[test]
    fn semantic_work_is_visible_in_monotonic_metric_deltas() {
        let mut engine = RustEngine::spawn().unwrap();
        let measured = measure(|| {
            execute_steps(
                &mut engine,
                "D(x)Limit(t,0)(Sin(t)/t+x^2)",
                StepVerbosity::Detailed,
            )
            .unwrap()
            .unwrap()
        });
        assert!(measured.metrics.parse_calls > 0);
        assert!(measured.metrics.engine_requests > 0);
        assert!(measured.metrics.object_transitions > 0);
        assert!(measured.metrics.object_clones > 0);
        assert!(!measured.value.steps.is_empty());
    }

    #[test]
    fn trace_off_does_not_materialize_rule_presentations() {
        let input = crate::elaboration::elaborate_input("D(x)Limit(t,0)(Sin(t)/t+x^2)").unwrap();
        let mut engine = RustEngine::spawn().unwrap();
        let off = measure(|| {
            crate::arithmetic::execute_elaborated_structure_with_context(
                &mut engine,
                &input.root,
                crate::semantic_core::ComputationContext::new(crate::semantic_core::TraceMode::Off),
            )
            .unwrap()
        });
        let detailed = measure(|| {
            crate::arithmetic::execute_elaborated_structure_with_context(
                &mut engine,
                &input.root,
                crate::semantic_core::ComputationContext::new(
                    crate::semantic_core::TraceMode::Detailed,
                ),
            )
            .unwrap()
        });
        assert_eq!(off.metrics.rule_presentations, 0);
        assert!(detailed.metrics.rule_presentations > 0);
        assert!(off
            .value
            .trace
            .unwrap()
            .events
            .iter()
            .all(|event| event.presentation.is_none()));
    }

    #[test]
    fn derivative_rule_facts_do_not_cross_the_legacy_trace_adapter() {
        let input = crate::elaboration::elaborate_input("D(x)(x^2)").unwrap();
        let mut engine = RustEngine::spawn().unwrap();
        let measured = measure(|| {
            crate::arithmetic::execute_elaborated_structure_with_context(
                &mut engine,
                &input.root,
                crate::semantic_core::ComputationContext::new(crate::semantic_core::TraceMode::Off),
            )
            .unwrap()
        });
        assert_eq!(measured.metrics.legacy_trace_adaptations, 0);
    }
}
