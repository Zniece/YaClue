//! Release-mode, persistent-session benchmark for the semantic adoption gate.
//!
//! Run with: `cargo run -p processing --release --example semantic_bench`

use std::hint::black_box;
use std::time::{Duration, Instant};

use processing::composition::execute_steps;
use processing::engine::{Engine, RustEngine};
use processing::improper_integrals::ImproperIntegralRequest;
use processing::intrinsics::try_lower_improper_integral;
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

    println!("persistent release medians ({SAMPLES} samples)");
    println!("direct_gamma_ns={}", direct.as_nanos());
    println!("standard_definition_ns={}", defined.as_nanos());
    println!("simple_equivalent_ns={}", equivalent_form.as_nanos());
    println!("recognition_miss_ns={}", recognition_miss.as_nanos());
    println!("long_composition_ns={}", long_chain.as_nanos());
}
