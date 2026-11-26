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

let root = use_xml::parse_str(xml)?;
let people = root
    .children("person")?
    .iter()
    .map(|p| Ok((p.attr("name", parse::string)?, p.attr_opt("address", parse::string)?)))
    .collect::<Result<Vec<_>, use_xml::Error>>()?;
root.check_all_used()?;
```

## Dokumentace

- [Návrh](docs/cs/design.md)
- [Plán](PLAN.cs.md)
- [Pravidla pro přispěvatele a agenty](AGENTS.cs.md)
