# Pravidla pro agenty

Anglická verze: [AGENTS.md](AGENTS.md).

Před začátkem jakékoli práce si přečti tento soubor a [PLAN.cs.md](PLAN.cs.md).
Podrobný návrh je v [docs/cs/design.md](docs/cs/design.md).

## Komunikace

- Se správcem mluv česky.
- Když si něčím nejsi jistý, zeptej se, nehádej.

## Jazyk

- Kód, identifikátory, komentáře, commit zprávy a chybové hlášky jsou v americké angličtině.
- Dokumentace je dvojjazyčná. Každý anglický dokument má český protějšek:
  - `README.md` ↔ `README.cs.md`
  - `PLAN.md` ↔ `PLAN.cs.md`
  - `AGENTS.md` ↔ `AGENTS.cs.md`
  - `docs/en/*` ↔ `docs/cs/*`
- Obě jazykové verze aktualizuj ve stejném commitu. Musí říkat totéž.

## Git

- Pracuj pouze ve větvi `master`. Nevytvářej jiné větve ani pull requesty.
- Commity drž malé a zaměřené. Každý commit se musí sestavit a projít kontrolami níže.

## Rust

- Používej nejnovější stable Rust a edici 2024.
- `#![forbid(unsafe_code)]`.
- Knihovna nesmí zpanikařit na žádném vstupním dokumentu ani při žádném použití API.
  Všechna selhání se hlásí přes `Result` s typem `Error` knihovny.
  V kódu knihovny nepoužívej `unwrap`/`expect`, pokud selhání nevylučuje nějaký invariant,
  a ten invariant zdokumentuj v komentáři.
- Každá veřejná položka má doc komentář. Důležité funkce mají příklady v dokumentaci.

## Závislosti

Povolené jsou pouze tyto závislosti, zafixované na přesné verze:

| Crate          | Verze     | Poznámka                                                    |
|----------------|-----------|-------------------------------------------------------------|
| `quick-xml`    | `=0.42.0` | Tokenizer XML.                                              |
| `thiserror`    | `=2.0.21` | Typ `Error` knihovny.                                       |
| `anyhow`       | `=1.0.105`| Pouze v testech a příkladech.                               |
| `rust_decimal` | `=1.43.0` | `default-features = false, features = ["macros"]`           |
| `time`         | `=0.3.55` | `default-features = false, features = ["formatting", "parsing", "macros"]` |

Bez výslovného svolení správce žádnou závislost nepřidávej, neodebírej ani neaktualizuj.

## Kontroly před každým commitem

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

## Testy

- Každé pravidlo z `docs/cs/design.md` má alespoň jeden test, včetně chybových případů.
- Převezmi relevantní testy z původní F# knihovny
  [UseData.Xml](https://github.com/radekm/UseData.Xml).
- Testy chyb kontrolují variantu chyby a hlášenou pozici, nejen to, že chyba nastala.

## Plán

- Když dokončíš krok z `PLAN.cs.md`, označ ho jako hotový v `PLAN.md` i `PLAN.cs.md`.
- Když se změní rozhodnutí v návrhu, nejdřív aktualizuj `docs/en/design.md` a `docs/cs/design.md`.
