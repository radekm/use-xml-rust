# Plan

Czech version: [PLAN.cs.md](PLAN.cs.md). Rules: [AGENTS.md](AGENTS.md). Design: [docs/en/design.md](docs/en/design.md).

## Goal

A Rust library for extracting data from XML, modeled on
[UseData.Xml](https://github.com/radekm/UseData.Xml).

## Steps

- [x] 0. Documentation and rules: `AGENTS.md`, `PLAN.md`, `README.md`, `docs/en/design.md` (+ Czech versions).
- [ ] 1. Project skeleton: `Cargo.toml` (edition 2024, pinned dependencies), `src/lib.rs`
      with `#![forbid(unsafe_code)]`, `.gitignore`, CI-like checks pass on an empty crate.
- [ ] 2. `Position` and the `Error` type with all variants from the design.
- [ ] 3. Parser into the `Elem` tree on top of `quick-xml` (`parse_str`):
      - local names, skipping `xmlns` attributes, duplicate attributes, child prefix mismatch,
      - text, CDATA, whitespace handling, significant text, mixed content error,
      - entity decoding, ignoring comments and processing instructions,
      - rules before and after the root element, `DOCTYPE` error,
      - `MAX_DEPTH = 256` without recursion,
      - line and column for every element and every error.
- [ ] 4. Extraction API: `name`, `position`, `attr`, `attr_opt`, `child`, `child_opt`, `children`,
      `text`, `ignore_all`, with usage tracking and "already used" errors.
- [ ] 5. `check_all_used` (depth-first, first unused item, order as in the design).
- [ ] 6. The `parse` module: strings, `one_of`, `enumeration`, integers, `decimal`,
      `decimal_non_negative`, `bool`, `date_time_utc`, `map`, `validate`.
- [ ] 7. Tests ported from UseData.Xml plus tests for every rule and error in the design.
- [ ] 8. Documentation: doc comments with examples, README usage example, `cargo doc` without warnings.
- [ ] 9. Optional: a parser from `std::io::BufRead` / bytes, benchmark (only if the maintainer wants it).

## Open questions

1. Integer parsing: strict Rust `from_str` (optional `+`/`-`, no whitespace),
   or .NET-like behavior allowing surrounding whitespace?
2. `bool`: accept only `true`/`false`, or also XML Schema `1`/`0`,
   or case-insensitive like .NET (`True`)?
3. `date_time_utc`: the original accepts `YYYY-MM-DDTHH:MM:SSZ` (or a space instead of `T`)
   with 1–7 fractional digits. Keep 7, or allow up to 9 (nanoseconds)?
   Should other offsets than `Z` be accepted?
4. `decimal`: the original allows only an optional leading sign and a decimal point
   (no exponent, no thousands separators, no whitespace). Keep this strictly?
5. Should errors also contain the element path (e.g. `/root/items/item[3]`) in addition to line and column?
