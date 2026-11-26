//! Benchmark of parsing and extracting an M7 message.
//!
//! Run with `cargo bench`. To see the overhead of `use-xml` itself,
//! compare `parse_m7_message` with `read_by_quick_xml`, which only tokenizes the message.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

mod m7;

/// Reads all events of the message similarly to `readByXmlReader` in UseData.Xml.
fn read_by_quick_xml(message: &str) -> usize {
    let mut i = 0usize;
    let mut reader = Reader::from_str(message);
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

fn benchmarks(c: &mut Criterion) {
    c.bench_function("parse_m7_message", |b| {
        b.iter(|| m7::parse_message(black_box(m7::MESSAGE)).expect("message is valid"))
    });
    c.bench_function("parse_str_only", |b| {
        b.iter(|| use_xml::parse_str(black_box(m7::MESSAGE)).expect("message is valid"))
    });
    c.bench_function("read_by_quick_xml", |b| {
        b.iter(|| read_by_quick_xml(black_box(m7::MESSAGE)))
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
