# Plán

Anglická verze: [PLAN.md](PLAN.md). Pravidla: [AGENTS.cs.md](AGENTS.cs.md). Návrh: [docs/cs/design.md](docs/cs/design.md).

## Cíl

Knihovna v Rustu pro extrakci dat z XML, inspirovaná
[UseData.Xml](https://github.com/radekm/UseData.Xml).

## Kroky

- [x] 0. Dokumentace a pravidla: `AGENTS.md`, `PLAN.md`, `README.md`, `docs/en/design.md` (+ české verze).
- [x] 1. Kostra projektu: `Cargo.toml` (edice 2024, zafixované závislosti), `src/lib.rs`
      s `#![forbid(unsafe_code)]`, `.gitignore`, kontroly projdou na prázdném crate.
- [x] 2. `Position` a typ `Error` se všemi variantami z návrhu.
- [x] 3. Parser do stromu `Elem` nad `quick-xml` (`parse_str`):
      - local names, přeskakování atributů `xmlns`, duplicitní atributy, různé prefixy dětí,
      - text, CDATA, zacházení s bílými znaky, významný text, chyba smíšeného obsahu,
      - dekódování entit, ignorování komentářů a processing instructions,
      - pravidla před a za kořenovým elementem, chyba `DOCTYPE`,
      - `MAX_DEPTH = 256` bez rekurze,
      - řádek a sloupec u každého elementu a každé chyby.
- [x] 4. API pro extrakci: `name`, `position`, `attr`, `attr_opt`, `child`, `child_opt`, `children`,
      `text`, `ignore_all`, se sledováním použití a chybami „already used“.
- [x] 5. `check_all_used` (do hloubky, první nepoužitá položka, pořadí podle návrhu).
- [x] 6. Modul `parse`: řetězce, `one_of`, `enumeration`, celá čísla, `decimal`,
      `decimal_non_negative`, `bool`, `date_time_utc`, `map`, `validate`.
- [x] 7. Testy převzaté z UseData.Xml a testy pro každé pravidlo a chybu z návrhu.
- [x] 8. Dokumentace: doc komentáře s příklady, příklad použití v README, `cargo doc` bez varování.
- [x] 9. Parser z bajtů (`parse_bytes`, jen UTF-8). `std::io::BufRead` se neplánuje,
      protože je synchronní.
- [x] 10. Benchmark převzatý z UseData.Xml pomocí criterion (`cargo bench`, viz `benches/`).

## Rozhodnutí

1. Celá čísla se parsují striktně rustovým `from_str` (volitelné `+`/`-`, žádné bílé znaky).
2. `bool` přijímá pouze `true` a `false`.
3. `date_time_utc` přijímá `YYYY-MM-DDTHH:MM:SSZ` (nebo mezeru místo `T`)
   s volitelným zlomkem sekund o 1–9 číslicích a pouze offset `Z`.
   Samostatný parser `date_time_offset` parsuje datum a čas s offsetem.
4. `decimal` je striktní: pouze volitelné znaménko na začátku a volitelná desetinná tečka
   (žádný exponent, oddělovače tisíců ani bílé znaky).
5. Chyby kromě řádku a sloupce obsahují i cestu k elementu (např. `/root/items[1]/item[3]`).

## Otevřené otázky

Momentálně žádné.
