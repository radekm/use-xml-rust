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
- [x] 8. Documentation: doc comments with examples, README usage example, `cargo doc` without warnings.
- [x] 9. A parser from bytes (`parse_bytes`, UTF-8 only). `std::io::BufRead` is not planned
      because it is synchronous.
- [x] 10. Benchmark ported from UseData.Xml using criterion (`cargo bench`, see `benches/`).

### Rewrite according to `NEW-SPEC.cs.md`

- [x] 11. Own checking parser of XML 1.0 and Namespaces 1.0 (`quick-xml` removed):
      names, characters, XML declaration, comments, processing instructions, CDATA,
      references, attribute value normalization, namespace declarations and reserved prefixes.
- [x] 12. Limits `MAX_NAME_LEN`, `MAX_PREFIX_LEN`, `MAX_NAMESPACE_LEN`, `MAX_NAMESPACES`,
      `MAX_ATTRS`, `MAX_CHILD_NAMES` with the error `LimitExceeded`.
- [x] 13. Interning of namespaces (`NamespaceHandle(u8)`) and qualified names
      (`QualifiedNameHandle(u32)`).
- [x] 14. `Document` with an arena of nodes and `Elem<'doc>` as a `Copy` handle;
      attribute values and texts are moved out on use, missing names are recorded in the maps.
- [x] 15. API with namespaces (`doc.namespace`, `attr(ns, name, parser)`, `child(ns, name)`, …),
      value parsers `FnOnce(String) -> Result<T, E>`.
- [x] 16. Tests, benchmark, and documentation updated.

## Decisions

1. Integers are parsed strictly with Rust `from_str` (optional `+`/`-`, no whitespace).
2. `bool` accepts only `true` and `false`.
3. `date_time_utc` accepts `YYYY-MM-DDTHH:MM:SSZ` (or a space instead of `T`)
   with an optional fraction of 1–9 digits, and only the `Z` offset.
   A separate parser `date_time_offset` parses date times with an offset.
4. `decimal` is strict: an optional leading sign and an optional decimal point only
   (no exponent, no thousands separators, no whitespace).
5. Errors contain the element path (e.g. `/root/items[1]/item[3]`) in addition to line and column.
6. `NamespaceHandle(u8)` (at most 256 namespaces including the empty one and `xml`),
   `QualifiedNameHandle(u32)`.
7. The XML parser is our own; `quick-xml` is no longer a dependency.
8. Elements live in an arena owned by `Document`; `Elem<'doc>` is a handle (document reference + index),
   the parent is an index. The lifetime guarantees that the document outlives its elements.
9. Value parsers take `String` (the value is moved out of the document), so `parse::string`
   does not copy. `InvalidValue` therefore does not contain the value;
   the parsers from `parse` put it (cut) into their messages.
10. Paths use names as written in the document (with prefixes); the index counts siblings
    with the same qualified name. Names in errors are `QualifiedName`, displayed as `local`
    or `{namespace}local`.
11. `check_all_used` orders attributes and child names by their handles, i.e. by the first
    occurrence of the qualified name in the document (previously alphabetically).
12. `MAX_NAME_LEN` applies to the local name (the prefix has its own limit),
    to processing instruction targets, and to entity names. `MAX_ATTRS` counts namespace declarations too.
13. Namespace-well-formedness errors are reported as `ErrorKind::Namespace`.
    `UnknownEntity` has no path (it may be in a start tag whose element does not exist yet).
14. Requested names are checked like names in the document: an invalid local name is
    `InvalidName`, an invalid namespace name in `Document::namespace` is `InvalidNamespace`
    (or `LimitExceeded` if it is too long). Namespace names are not checked to be URIs.
15. `MixedContent` points to the significant text (before or after the child elements),
    not to the element.

## Open questions

None at the moment.
