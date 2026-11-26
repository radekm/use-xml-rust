# use-xml

Anglická verze: [README.md](README.md).

Knihovna v Rustu pro extrakci dat z XML dokumentů, inspirovaná F# knihovnou
[UseData.Xml](https://github.com/radekm/UseData.Xml).

**Stav:** jádro je hotové, API se ještě může změnit. Viz [PLAN.cs.md](PLAN.cs.md).

## Myšlenka

Dokument se naparsuje do stromu elementů. Vyžádáš si atributy, děti a text
a převedeš je pomocí parserů hodnot. Každou část dokumentu lze použít jen jednou.
Na konci zavoláš `check_all_used`, která selže, pokud nějaký atribut, text nebo dítě
zůstal nepoužitý. Změny ve formátu vstupu se tak tiše neignorují.

```rust
use use_xml::parse;

fn main() -> Result<(), use_xml::Error> {
    let xml = r#"
        <people>
            <person name="Pete" />
            <person name="Harry" address="Little Whinging" />
        </people>
    "#;
    let root = use_xml::parse_str(xml)?;
    let people = root
        .children("person")?
        .iter()
        .map(|p| Ok((p.attr("name", parse::string)?, p.attr_opt("address", parse::string)?)))
        .collect::<Result<Vec<_>, use_xml::Error>>()?;
    root.check_all_used()?;
    assert_eq!(people.len(), 2);
    Ok(())
}
```

## Benchmark

`cargo bench` parsuje zprávu s order bookem ve formátu M7 (převzato z UseData.Xml) a porovnává
`parse_m7_message` (parsování, extrakce, `check_all_used`), `parse_str_only`
a `read_by_quick_xml` (pouze tokenizace pomocí `quick-xml`).
Benchmark používá [criterion](https://crates.io/crates/criterion).

## Dokumentace

- [Návrh](docs/cs/design.md)
- [Plán](PLAN.cs.md)
- [Pravidla pro přispěvatele a agenty](AGENTS.cs.md)

## Licence

[zlib](LICENSE), Copyright (c) 2026 Radek Micek.
