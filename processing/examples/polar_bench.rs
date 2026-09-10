//! Release-mode, persistent-session benchmark for polar normalization.
//!
//! Run with: `cargo run -p processing --release --example polar_bench`

use std::hint::black_box;
use std::time::{Duration, Instant};

use processing::engine::RustEngine;
use processing::multiple_integrals::{polar_integral, PolarRegion};

const WARMUPS: usize = 3;
const SAMPLES: usize = 9;

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
    let region = PolarRegion {
        radial_lower: "0",
        radial_upper: "2",
        angle_lower: "0",
        angle_upper: "2*Pi",
    };
    let mut polar = |expression: &str| {
        black_box(
            polar_integral(&mut engine, expression, "x", "y", "r", "theta", region)
                .expect("polar integral"),
        );
    };
    let constant = measure(|| polar("1"));
    let quadratic = measure(|| polar("x^2+y^2"));
    let gaussian = measure(|| polar("Exp(-(x^2+y^2))"));
    let unresolved = measure(|| polar("Exp(x)"));

    println!("persistent release medians ({SAMPLES} samples)");
    println!("constant_disk_ns={}", constant.as_nanos());
    println!("quadratic_disk_ns={}", quadratic.as_nanos());
    println!("gaussian_disk_ns={}", gaussian.as_nanos());
    println!("unresolved_exponential_disk_ns={}", unresolved.as_nanos());
}
