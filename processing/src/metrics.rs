//! Low-overhead, monotonic counters for semantic-pipeline baselines.
//!
//! These counters describe work rather than wall-clock policy. Consumers take
//! two snapshots around a single-threaded scenario and subtract them; normal
//! execution never resets shared process state.

use std::cell::Cell;
use std::ops::Sub;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static ENGINE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static OBJECT_TRANSITIONS: AtomicU64 = AtomicU64::new(0);
static OBJECT_CLONES: AtomicU64 = AtomicU64::new(0);
static SESSION_AST_HANDLES: AtomicU64 = AtomicU64::new(0);
static RULE_PRESENTATIONS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Per-execution-thread counters used by `measure`. Global atomics remain
    /// the production observability source; this mirror prevents unrelated
    /// parallel requests from contaminating one measured scenario.
    static THREAD_METRICS: Cell<ExecutionMetrics> = const { Cell::new(ExecutionMetrics::ZERO) };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionMetrics {
    pub parse_calls: u64,
    pub engine_requests: u64,
    pub object_transitions: u64,
    pub object_clones: u64,
    pub session_ast_handles: u64,
    pub rule_presentations: u64,
}

impl ExecutionMetrics {
    const ZERO: Self = Self {
        parse_calls: 0,
        engine_requests: 0,
        object_transitions: 0,
        object_clones: 0,
        session_ast_handles: 0,
        rule_presentations: 0,
    };

    pub fn snapshot() -> Self {
        Self {
            parse_calls: yacas_rs::parser::parse_call_count(),
            engine_requests: ENGINE_REQUESTS.load(Ordering::Relaxed),
            object_transitions: OBJECT_TRANSITIONS.load(Ordering::Relaxed),
            object_clones: OBJECT_CLONES.load(Ordering::Relaxed),
            session_ast_handles: SESSION_AST_HANDLES.load(Ordering::Relaxed),
            rule_presentations: RULE_PRESENTATIONS.load(Ordering::Relaxed),
        }
    }

    fn thread_snapshot() -> Self {
        THREAD_METRICS.with(Cell::get)
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
    let before = ExecutionMetrics::thread_snapshot();
    let parse_calls_before = yacas_rs::parser::parse_call_count();
    let started = Instant::now();
    let value = operation();
    Measured {
        value,
        metrics: ExecutionMetrics {
            parse_calls: yacas_rs::parser::parse_call_count().saturating_sub(parse_calls_before),
            ..(ExecutionMetrics::thread_snapshot() - before)
        },
        elapsed: started.elapsed(),
    }
}

fn record_thread_metric(update: impl FnOnce(&mut ExecutionMetrics)) {
    THREAD_METRICS.with(|metrics| {
        let mut current = metrics.get();
        update(&mut current);
        metrics.set(current);
    });
}

pub(crate) fn record_engine_request() {
    ENGINE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    record_thread_metric(|metrics| metrics.engine_requests += 1);
}

pub(crate) fn record_object_transition() {
    OBJECT_TRANSITIONS.fetch_add(1, Ordering::Relaxed);
    record_thread_metric(|metrics| metrics.object_transitions += 1);
}

pub(crate) fn record_object_clone() {
    OBJECT_CLONES.fetch_add(1, Ordering::Relaxed);
    record_thread_metric(|metrics| metrics.object_clones += 1);
}

pub(crate) fn record_session_ast_handle() {
    SESSION_AST_HANDLES.fetch_add(1, Ordering::Relaxed);
    record_thread_metric(|metrics| metrics.session_ast_handles += 1);
}

pub(crate) fn record_rule_presentation() {
    RULE_PRESENTATIONS.fetch_add(1, Ordering::Relaxed);
    record_thread_metric(|metrics| metrics.rule_presentations += 1);
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
}
