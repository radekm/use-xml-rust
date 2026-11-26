# Plan

Czech version: [PLAN.cs.md](PLAN.cs.md). Rules: [AGENTS.md](AGENTS.md). Design: [docs/en/design.md](docs/en/design.md).

## Goal

A Rust library for extracting data from XML, modeled on
[UseData.Xml](https://github.com/radekm/UseData.Xml).

## Steps

- [x] 0. Documentation and rules: `AGENTS.md`, `PLAN.md`, `README.md`, `docs/en/design.md` (+ Czech versions).
- [x] 1. Project skeleton: `Cargo.toml` (edition 2024, pinned dependencies), `src/lib.rs`
      with `#![forbid(unsafe_code)]`, `.gitignore`, CI-like checks pass on an empty crate.
- [x] 2. `Position` and the `Error` type with all variants from the design.
- [x] 3. Parser into the `Elem` tree on top of `quick-xml` (`parse_str`):
      - local names, skipping `xmlns` attributes, duplicate attributes, child prefix mismatch,
      - text, CDATA, whitespace handling, significant text, mixed content error,
      - entity decoding, ignoring comments and processing instructions,
      - rules before and after the root element, `DOCTYPE` error,
      - `MAX_DEPTH = 256` without recursion,
      - line and column for every element and every error.
- [x] 4. Extraction API: `name`, `position`, `attr`, `attr_opt`, `child`, `child_opt`, `children`,
      `text`, `ignore_all`, with usage tracking and "already used" errors.
- [x] 5. `check_all_used` (depth-first, first unused item, order as in the design).
- [x] 6. The `parse` module: strings, `one_of`, `enumeration`, integers, `decimal`,
      `decimal_non_negative`, `bool`, `date_time_utc`, `map`, `validate`.
- [x] 7. Tests ported from UseData.Xml plus tests for every rule and error in the design.
- [ ] 8. Documentation: doc comments with examples, README usage example, `cargo doc` without warnings.
- [x] 9. A parser from bytes (`parse_bytes`, UTF-8 only). `std::io::BufRead` is not planned
      because it is synchronous.
- [x] 10. Benchmark ported from UseData.Xml using criterion (`cargo bench`, see `benches/`).

## Decisions

1. Integers are parsed strictly with Rust `from_str` (optional `+`/`-`, no whitespace).
2. `bool` accepts only `true` and `false`.
3. `date_time_utc` accepts `YYYY-MM-DDTHH:MM:SSZ` (or a space instead of `T`)
   with an optional fraction of 1–9 digits, and only the `Z` offset.
   A separate parser `date_time_offset` parses date times with an offset.
4. `decimal` is strict: an optional leading sign and an optional decimal point only
   (no exponent, no thousands separators, no whitespace).
5. Errors contain the element path (e.g. `/root/items[1]/item[3]`) in addition to line and column.

## Open questions

None at the moment.
