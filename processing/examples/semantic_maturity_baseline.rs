//! Single-session semantic-pipeline baseline for maturity work.
//!
//! Run in release mode and preserve the output with the E1 audit record:
//! `cargo run -p processing --release --example semantic_maturity_baseline`

use processing::composition::execute_elaborated;
use processing::elaboration::elaborate_input;
use processing::engine::RustEngine;
use processing::metrics::{measure, ExecutionMetrics};
use processing::steps::StepVerbosity;

const CASES: &[&str] = &[
    "Limit(Sin(x)/x,0)",
    "Taylor(Exp(x),0,6)",
    "D(X)Integrate(t,0,X)Sin(t)",
    "Subst(X,1)Integrate(t,0,X)Sin(t)",
    "Solve({x+y==3,x-y==1},{x,y})",
    "Determinant({{1,2},{3,4}})",
    "D(x)Limit(t,0)(Sin(t)/t+x^2)",
    "D(x)Limit(t,0)(1/t)",
    "Plot(D(x)(x^2),x,0,1)",
];

fn run(
    engine: &mut RustEngine,
    source: &str,
    include_steps: bool,
) -> processing::metrics::Measured<processing::composition::CompositionResult> {
    measure(|| {
        let input = elaborate_input(source).expect("baseline input elaborates");
        execute_elaborated(engine, &input, StepVerbosity::Detailed, include_steps)
            .expect("baseline input executes")
            .expect("complete input has a product result")
    })
}

fn print_metrics(source: &str, mode: &str, metrics: ExecutionMetrics, micros: u128, events: usize) {
    println!(
        "{source}\t{mode}\t{micros}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{events}",
        metrics.parse_calls,
        metrics.engine_requests,
        metrics.object_transitions,
        metrics.object_clones,
        metrics.session_ast_handles,
        metrics.rule_presentations,
        metrics.legacy_trace_adaptations,
    );
}

fn main() {
    let mut engine = RustEngine::spawn().expect("persistent engine");
    println!(
        "expression\tmode\tmicros\tparses\tengine_requests\ttransitions\tobject_clones\tsession_ast_handles\trule_presentations\tlegacy_trace_adaptations\tvisible_events"
    );
    for source in CASES {
        let off = run(&mut engine, source, false);
        let detailed = run(&mut engine, source, true);
        assert_eq!(off.value.value, detailed.value.value, "{source}");
        assert_eq!(off.value.status, detailed.value.status, "{source}");
        assert_eq!(off.value.semantic, detailed.value.semantic, "{source}");
        assert_eq!(off.value.outcome, detailed.value.outcome, "{source}");
        assert_eq!(off.value.conditions, detailed.value.conditions, "{source}");
        print_metrics(
            source,
            "off",
            off.metrics,
            off.elapsed.as_micros(),
            off.value.steps.len() + off.value.analyses.len() + off.value.conclusions.len(),
        );
        print_metrics(
            source,
            "detailed",
            detailed.metrics,
            detailed.elapsed.as_micros(),
            detailed.value.steps.len()
                + detailed.value.analyses.len()
                + detailed.value.conclusions.len(),
        );
    }
}
