# Rules for agents

Czech version: [AGENTS.cs.md](AGENTS.cs.md).

Read this file and [PLAN.md](PLAN.md) before starting any work.
The detailed design is in [docs/en/design.md](docs/en/design.md).

## Communication

- Talk to the maintainer in Czech.
- When you are not sure about something, ask instead of guessing.

## Language

- Code, identifiers, comments, commit messages, and error messages are in American English.
- Documentation is bilingual. Every English document has a Czech counterpart:
  - `README.md` ↔ `README.cs.md`
  - `PLAN.md` ↔ `PLAN.cs.md`
  - `AGENTS.md` ↔ `AGENTS.cs.md`
  - `docs/en/*` ↔ `docs/cs/*`
- Update both language versions in the same commit. They must say the same thing.

## Git

- Work only on the `master` branch. Do not create other branches or pull requests.
- Keep commits small and focused. Each commit must build and pass the checks below.

## Rust

- Use the latest stable Rust and edition 2024. The version is pinned in `rust-toolchain.toml`;
  update it (together with fixing new warnings) when a new stable version is released.
- `#![forbid(unsafe_code)]`.
- The library must not panic on any input document or any API usage.
  All failures are reported through `Result` with the library's `Error` type.
  Do not use `unwrap`/`expect` in library code unless an invariant makes failure impossible,
  and document that invariant in a comment.
- Every public item has a doc comment. Important functions have doc examples.

## Dependencies

Only these dependencies are allowed, pinned to exact versions:

| Crate          | Version   | Notes                                                       |
|----------------|-----------|-------------------------------------------------------------|
| `quick-xml`    | `=0.42.0` | XML tokenizer.                                              |
| `thiserror`    | `=2.0.21` | The library `Error` type.                                   |
| `anyhow`       | `=1.0.105`| Only in tests and examples.                                 |
| `criterion`    | `=0.8.2`  | Only in benchmarks; `default-features = false, features = ["cargo_bench_support"]` |
| `rust_decimal` | `=1.43.0` | `default-features = false`; feature `macros` only in `[dev-dependencies]` |
| `time`         | `=0.3.55` | `default-features = false`; features `formatting`, `macros` only in `[dev-dependencies]` |

Do not add, remove, or upgrade a dependency without the maintainer's explicit permission.

## Checks before every commit

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

## Tests

- Every rule from `docs/en/design.md` has at least one test, including the error cases.
- Port the relevant tests from the original F# library
  [UseData.Xml](https://github.com/radekm/UseData.Xml).
- Error tests check the error variant and the reported position, not only that an error occurred.

## Plan

- When you finish a step of `PLAN.md`, mark it as done in both `PLAN.md` and `PLAN.cs.md`.
- When a design decision changes, update `docs/en/design.md` and `docs/cs/design.md` first.
