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

### Přepis podle `NEW-SPEC.cs.md`

- [x] 11. Vlastní kontrolující parser XML 1.0 a Namespaces 1.0 (`quick-xml` odstraněn):
      jména, znaky, XML deklarace, komentáře, processing instructions, CDATA,
      reference, normalizace hodnot atributů, deklarace namespaců a rezervované prefixy.
- [x] 12. Limity `MAX_NAME_LEN`, `MAX_PREFIX_LEN`, `MAX_NAMESPACE_LEN`, `MAX_NAMESPACES`,
      `MAX_ATTRS`, `MAX_CHILD_NAMES` s chybou `LimitExceeded`.
- [x] 13. Internování namespaců (`NamespaceHandle(u8)`) a kvalifikovaných jmen
      (`QualifiedNameHandle(u32)`).
- [x] 14. `Document` s arénou uzlů a `Elem<'doc>` jako `Copy` handle;
      hodnoty atributů a texty se při použití přesunou ven, chybějící jména se zaznamenají do map.
- [x] 15. API s namespacy (`doc.namespace`, `attr(ns, name, parser)`, `child(ns, name)`, …),
      parsery hodnot `FnOnce(String) -> Result<T, E>`.
- [x] 16. Aktualizované testy, benchmark a dokumentace.

## Rozhodnutí

1. Celá čísla se parsují striktně rustovým `from_str` (volitelné `+`/`-`, žádné bílé znaky).
2. `bool` přijímá pouze `true` a `false`.
3. `date_time_utc` přijímá `YYYY-MM-DDTHH:MM:SSZ` (nebo mezeru místo `T`)
   s volitelným zlomkem sekund o 1–9 číslicích a pouze offset `Z`.
   Samostatný parser `date_time_offset` parsuje datum a čas s offsetem.
4. `decimal` je striktní: pouze volitelné znaménko na začátku a volitelná desetinná tečka
   (žádný exponent, oddělovače tisíců ani bílé znaky).
5. Chyby kromě řádku a sloupce obsahují i cestu k elementu (např. `/root/items[1]/item[3]`).
6. `NamespaceHandle(u8)` (nejvýše 256 namespaců včetně prázdného a `xml`),
   `QualifiedNameHandle(u32)`.
7. Parser XML je vlastní; `quick-xml` už není závislost.
8. Elementy žijí v aréně, kterou vlastní `Document`; `Elem<'doc>` je handle (reference na dokument + index),
   rodič je index. Lifetime zaručuje, že dokument přežije své elementy.
9. Parsery hodnot dostávají `String` (hodnota se z dokumentu přesune), takže `parse::string`
   nekopíruje. `InvalidValue` proto hodnotu neobsahuje;
   parsery z `parse` ji (zkrácenou) vkládají do svých hlášek.
10. Cesty používají jména tak, jak jsou zapsaná v dokumentu (s prefixy); index počítá sourozence
    se stejným kvalifikovaným jménem. Jména v chybách jsou `QualifiedName`, zobrazená jako `local`
    nebo `{namespace}local`.
11. `check_all_used` řadí atributy a jména dětí podle jejich handlů, tj. podle prvního
    výskytu kvalifikovaného jména v dokumentu (dříve abecedně).
12. `MAX_NAME_LEN` platí pro lokální jméno (prefix má vlastní limit),
    pro cíle processing instructions a pro jména entit. `MAX_ATTRS` započítává i deklarace namespaců.
13. Chyby namespace-well-formedness se hlásí jako `ErrorKind::Namespace`.
    `UnknownEntity` nemá cestu (může být v počátečním tagu elementu, který ještě neexistuje).

## Otevřené otázky

Momentálně žádné.
