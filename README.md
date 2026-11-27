# use-xml

Czech version: [README.cs.md](README.cs.md).

A Rust library for extracting data from XML documents, modeled on the F# library
[UseData.Xml](https://github.com/radekm/UseData.Xml).

**Status:** the core is implemented, the API may still change. See [PLAN.md](PLAN.md).

## Idea

The document is parsed by a strict parser of XML 1.0 with namespaces into a `Document`.
You request attributes, children, and text by a namespace and a local name,
and convert them with value parsers. Each piece of the document can be used only once.
At the end you call `check_all_used`, which fails if some attribute, text, or child
was not used, so changes in the input format are not silently ignored.

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
    // Namespaces which do not exist in the document are interned too.
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

`cargo bench` parses an M7 order book message (ported from UseData.Xml) and compares
`parse_m7_message` (parsing, extraction, `check_all_used`) with `parse_str_only`.
The benchmark uses [criterion](https://crates.io/crates/criterion).

## Documentation

- [Design](docs/en/design.md)
- [Plan](PLAN.md)
- [Rules for contributors and agents](AGENTS.md)

## License

[zlib](LICENSE), Copyright (c) 2026 Radek Micek.
