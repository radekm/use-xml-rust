//! Benchmark of parsing and extracting an M7 message.
//!
//! Run with `cargo bench`. To see the cost of the extraction itself,
//! compare `parse_m7_message` with `parse_str_only`, which only parses the message.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

mod m7;

fn benchmarks(c: &mut Criterion) {
    c.bench_function("parse_m7_message", |b| {
        b.iter(|| m7::parse_message(black_box(m7::MESSAGE)).expect("message is valid"))
    });
    c.bench_function("parse_str_only", |b| {
        b.iter(|| use_xml::parse_str(black_box(m7::MESSAGE)).expect("message is valid"))
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
