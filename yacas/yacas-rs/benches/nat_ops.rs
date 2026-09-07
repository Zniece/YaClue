use std::hint::black_box;
use std::time::{Duration, Instant};

use yacas_rs::number::nat::Nat;

fn decimal(digits: usize, seed: usize) -> String {
    (0..digits)
        .map(|i| {
            let digit = if i == 0 { 1 + seed % 9 } else { (i + seed) % 10 };
            char::from_digit(digit as u32, 10).expect("decimal digit")
        })
        .collect()
}

fn gcd(mut left: Nat, mut right: Nat) -> Nat {
    while !right.is_zero() {
        let (_, remainder) = left.divrem(&right).expect("nonzero divisor");
        left = right;
        right = remainder;
    }
    left
}

fn measure(mut operation: impl FnMut()) -> (u32, Duration) {
    let start = Instant::now();
    operation();
    let once = start.elapsed().max(Duration::from_nanos(1));
    let repetitions = (Duration::from_millis(150).as_nanos() / once.as_nanos())
        .clamp(1, 1_000) as u32;
    let start = Instant::now();
    for _ in 0..repetitions {
        operation();
    }
    (repetitions, start.elapsed())
}

fn report(name: &str, digits: usize, repetitions: u32, elapsed: Duration) {
    let nanos = elapsed.as_nanos() / u128::from(repetitions);
    println!("{name},{digits},{repetitions},{nanos}");
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if !arguments.iter().any(|arg| arg == "--bench") {
        return;
    }
    let sizes: Vec<usize> = arguments
        .into_iter()
        .filter(|arg| arg != "--bench")
        .map(|arg| arg.parse().expect("sizes must be decimal integers"))
        .collect();
    let sizes = if sizes.is_empty() { vec![100, 1_000, 10_000] } else { sizes };

    println!("operation,digits,repetitions,ns_per_op");
    for digits in sizes {
        let left = Nat::from_decimal(&decimal(digits, 3)).expect("left operand");
        let right = Nat::from_decimal(&decimal(digits, 7)).expect("right operand");
        let dividend = Nat::from_decimal(&decimal(digits * 2, 5)).expect("dividend");

        let (repetitions, elapsed) = measure(|| {
            black_box(left.mul(&right));
        });
        report("mul_n_by_n", digits, repetitions, elapsed);

        let (repetitions, elapsed) = measure(|| {
            black_box(left.mul_pow10(digits as u32));
        });
        report("mul_pow10", digits, repetitions, elapsed);

        let (repetitions, elapsed) = measure(|| {
            black_box(dividend.divrem(&right).expect("division"));
        });
        report("divrem_2n_by_n", digits, repetitions, elapsed);

        let (repetitions, elapsed) = measure(|| {
            black_box(gcd(left.clone(), right.clone()));
        });
        report("gcd", digits, repetitions, elapsed);

        let (repetitions, elapsed) = measure(|| {
            black_box(left.bit_len());
        });
        report("bit_len", digits, repetitions, elapsed);

        let two = Nat::from_decimal("2").expect("two");
        let (repetitions, elapsed) = measure(|| {
            black_box(two.pow(digits as u32));
        });
        report("pow_2", digits, repetitions, elapsed);
    }
}
