//! Flattening performance baseline.

#![allow(missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use path_kit::{Path, Vec2};

/// Build a chain of `n` cubics.
fn chain_path(n: usize) -> Path {
    let mut p = Path::new();
    p.move_to(Vec2::ZERO);
    for i in 0..n {
        let x = f64::from(i as u32);
        p.cubic_to(
            Vec2::new(x + 1.0, 20.0),
            Vec2::new(x + 2.0, -20.0),
            Vec2::new(x + 3.0, 0.0),
        );
    }
    p
}

/// Flatten throughput at two tolerances.
fn bench_flatten(c: &mut Criterion) {
    let p = chain_path(500);
    c.bench_function("flatten_500_cubics_tol_0.25", |b| {
        b.iter(|| p.flatten(std::hint::black_box(0.25)))
    });
    c.bench_function("flatten_500_cubics_tol_0.01", |b| {
        b.iter(|| p.flatten(std::hint::black_box(0.01)))
    });
}

criterion_group!(benches, bench_flatten);
criterion_main!(benches);
