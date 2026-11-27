# use-xml

Anglická verze: [README.md](README.md).

Knihovna v Rustu pro extrakci dat z XML dokumentů, inspirovaná F# knihovnou
[UseData.Xml](https://github.com/radekm/UseData.Xml).

**Stav:** jádro je hotové, API se ještě může změnit. Viz [PLAN.cs.md](PLAN.cs.md).

## Myšlenka

Dokument se striktním parserem XML 1.0 s namespacy naparsuje do `Document`.
Atributy, děti a text si vyžádáš podle namespacu a lokálního jména
a převedeš je pomocí parserů hodnot. Každou část dokumentu lze použít jen jednou.
Na konci zavoláš `check_all_used`, která selže, pokud nějaký atribut, text nebo dítě
zůstal nepoužitý. Změny ve formátu vstupu se tak tiše neignorují.

```rust
use use_xml::parse;

fn main() -> Result<(), use_xml::Error> {
    let xml = r#"
        <x xmlns:edi='http://ecommerce.example.org/schema'>
            <lineItem edi:taxClass="exempt">Baby food</lineItem>
            <lineItem edi:taxClass="standard">Milk</lineItem>
        </x>
    "#;
    let doc = use_xml::parse_str(xml)?;
    // Namespacy, které v dokumentu nejsou, se internují také.
    let default_ns = doc.namespace("")?;
    let edi_ns = doc.namespace("http://ecommerce.example.org/schema")?;
    let items = doc
        .root()
        .children(default_ns, "lineItem")?
        .into_iter()
        .map(|item| Ok((item.text(parse::string)?, item.attr(edi_ns, "taxClass", parse::string)?)))
        .collect::<Result<Vec<_>, use_xml::Error>>()?;
    doc.check_all_used()?;
    assert_eq!(items[0], ("Baby food".to_owned(), "exempt".to_owned()));
    Ok(())
}
```

## Benchmark

`cargo bench` parsuje zprávu s order bookem ve formátu M7 (převzato z UseData.Xml) a porovnává
`parse_m7_message` (parsování, extrakce, `check_all_used`) s `parse_str_only`.
Benchmark používá [criterion](https://crates.io/crates/criterion).

## Dokumentace

- [Návrh](docs/cs/design.md)
- [Plán](PLAN.cs.md)
- [Pravidla pro přispěvatele a agenty](AGENTS.cs.md)

## Licence

[zlib](LICENSE), Copyright (c) 2026 Radek Micek.
