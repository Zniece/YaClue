//! Release-mode, persistent-session benchmark for the semantic adoption gate.
//!
//! Run with: `cargo run -p processing --release --example semantic_bench`
//! On macOS, capture peak RSS with:
//! `cargo build -p processing --release --example semantic_bench && /usr/bin/time -l target/release/examples/semantic_bench`
//! On Linux, use `/usr/bin/time -v` for the second command.

use std::hint::black_box;
use std::time::{Duration, Instant};

use processing::algebra::{TransformKind, TransformOperation, TransformRequest};
use processing::composition::execute_steps;
use processing::elaboration::elaborate;
use processing::engine::{Engine, RustEngine};
use processing::improper_integrals::ImproperIntegralRequest;
use processing::intrinsics::try_lower_improper_integral;
use processing::semantic_core::{OperationCacheBudget, SemanticOperation};
use processing::steps::StepVerbosity;

const WARMUPS: usize = 5;
const SAMPLES: usize = 21;

fn request(expression: &str) -> ImproperIntegralRequest {
    ImproperIntegralRequest {
        expression: expression.into(),
        variable: "t".into(),
        lower: "0".into(),
        upper: "Infinity".into(),
        singular_points: Vec::new(),
    }
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn measure(mut operation: impl FnMut()) -> Duration {
    for _ in 0..WARMUPS {
        operation();
    }
    median(
        (0..SAMPLES)
            .map(|_| {
                let started = Instant::now();
                operation();
                started.elapsed()
            })
            .collect(),
    )
}

fn main() {
    let mut engine = RustEngine::spawn().expect("persistent engine");
    let direct = measure(|| {
        black_box(engine.eval("Gamma(1/x)").expect("direct Gamma"));
    });
    let definition = request("t^(1/x-1)*Exp(-t)");
    let defined = measure(|| {
        black_box(
            try_lower_improper_integral(&mut engine, &definition, None)
                .expect("definition")
                .expect("definition match"),
        );
    });
    let equivalent = request("3*Exp(-(2*t))*t^a/t");
    let equivalent_form = measure(|| {
        black_box(
            try_lower_improper_integral(&mut engine, &equivalent, None)
                .expect("equivalent form")
                .expect("equivalent match"),
        );
    });
    let miss = request("Sin(t)/(1+t^2)");
    let recognition_miss = measure(|| {
        black_box(try_lower_improper_integral(&mut engine, &miss, None).expect("bounded miss"));
    });
    let long_chain = measure(|| {
        black_box(
            execute_steps(
                &mut engine,
                "D(x)D(x)D(x)D(x)D(x)x^8",
                StepVerbosity::Concise,
            )
            .expect("long composition"),
        );
    });
    let closed_special_derivative = measure(|| {
        black_box(
            execute_steps(&mut engine, "D(x)Beta(Sin(x),x^2)", StepVerbosity::Concise)
                .expect("closed special derivative"),
        );
    });
    let formal_special_derivative = measure(|| {
        black_box(
            execute_steps(
                &mut engine,
                "1+D(x)HypergeometricPFQ({a,b},{c},Sin(x))",
                StepVerbosity::Concise,
            )
            .expect("formal special derivative"),
        );
    });
    let calculus_chain = measure(|| {
        black_box(
            execute_steps(
                &mut engine,
                "D(x)Limit(t,0)(Sin(t)/t+x^4)",
                StepVerbosity::Concise,
            )
            .expect("calculus chain"),
        );
    });
    let geometry_chain = measure(|| {
        black_box(
            execute_steps(
                &mut engine,
                "D(a)(a*ScalarSurfaceIntegral(1,{x,y,z},{u,v,0},{u,v},{0,0},{2,3}))",
                StepVerbosity::Concise,
            )
            .expect("geometry chain"),
        );
    });

    let cache_input = elaborate("(x+1)^4").expect("cache input").object;
    let mut max_stable_representations = cache_input.stable_representation_count();
    for kind in [
        TransformKind::Expand,
        TransformKind::Factor,
        TransformKind::Simplify,
        TransformKind::Tidy,
    ] {
        let computation = TransformOperation
            .compute(
                &mut engine,
                &cache_input,
                &TransformRequest {
                    kind,
                    variable: None,
                },
            )
            .expect("cached transform");
        max_stable_representations = max_stable_representations.max(
            computation
                .subject()
                .expect("transform subject")
                .stable_representation_count(),
        );
    }
    let cached_operations = cache_input.cached_operation_count();
    let cache_budget = OperationCacheBudget::default();
    assert!(cached_operations <= cache_budget.max_entries);
    assert!(max_stable_representations <= 4);

    println!("persistent release medians ({SAMPLES} samples)");
    println!("direct_gamma_ns={}", direct.as_nanos());
    println!("standard_definition_ns={}", defined.as_nanos());
    println!("simple_equivalent_ns={}", equivalent_form.as_nanos());
    println!("recognition_miss_ns={}", recognition_miss.as_nanos());
    println!("long_composition_ns={}", long_chain.as_nanos());
    println!(
        "closed_special_derivative_ns={}",
        closed_special_derivative.as_nanos()
    );
    println!(
        "formal_special_derivative_ns={}",
        formal_special_derivative.as_nanos()
    );
    println!("calculus_chain_ns={}", calculus_chain.as_nanos());
    println!("geometry_chain_ns={}", geometry_chain.as_nanos());
    println!("cached_operations={cached_operations}");
    println!("max_stable_representations={max_stable_representations}");
}
