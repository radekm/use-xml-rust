//! Benchmark of parsing and extracting an M7 message.
//!
//! Run with `cargo bench`. To see the overhead of `use-xml` itself,
//! compare `parse_m7_message` with `read_by_quick_xml`, which only tokenizes the message.

use std::hint::black_box;
use std::time::{Duration, Instant};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

mod m7;

/// Reads all events of the message similarly to `readByXmlReader` in UseData.Xml.
fn read_by_quick_xml() -> usize {
    let mut i = 0usize;
    let mut reader = Reader::from_str(m7::MESSAGE);
    loop {
        match reader.read_event().expect("valid XML") {
            Event::Start(e) | Event::Empty(e) => {
                i += e.name().as_ref().len();
                for attr in e.attributes() {
                    let attr = attr.expect("valid attribute");
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .expect("valid attribute value");
                    i += attr.key.as_ref().len() + value.len();
                }
            }
            Event::End(e) => i = i.wrapping_sub(e.name().as_ref().len()),
            Event::Text(t) => i += t.xml10_content().len(),
            Event::CData(t) => i += t.xml10_content().len(),
            Event::Eof => break,
            _ => {}
        }
    }
    i
}

/// Runs `f` repeatedly and prints the mean and the best time per iteration.
fn bench<T>(name: &str, mut f: impl FnMut() -> T) {
    const SAMPLES: usize = 20;
    const SAMPLE_TIME: Duration = Duration::from_millis(100);

    // Warm up and estimate the number of iterations per sample.
    let start = Instant::now();
    let mut iterations = 0u64;
    while start.elapsed() < SAMPLE_TIME {
        black_box(f());
        iterations += 1;
    }

    let mut per_iter = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(f());
        }
        per_iter.push(start.elapsed().as_secs_f64() / iterations as f64);
    }
    let mean = per_iter.iter().sum::<f64>() / SAMPLES as f64;
    let best = per_iter.iter().copied().fold(f64::INFINITY, f64::min);
    println!(
        "{name:<20} mean {:>9.2} µs   best {:>9.2} µs   ({SAMPLES} samples × {iterations} iterations)",
        mean * 1e6,
        best * 1e6
    );
}

fn main() {
    // Without `--bench` (e.g. `cargo test --benches`) only check that the extraction works.
    let deltas = m7::parse_message().expect("message is valid");
    assert_eq!(deltas.len(), 14);
    if !std::env::args().any(|a| a == "--bench") {
        return;
    }

    bench("parse_m7_message", || {
        m7::parse_message().expect("message is valid")
    });
    bench("parse_str_only", || {
        use_xml::parse_str(m7::MESSAGE).expect("message is valid")
    });
    bench("read_by_quick_xml", read_by_quick_xml);
}
