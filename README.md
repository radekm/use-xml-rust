# use-xml

Czech version: [README.cs.md](README.cs.md).

A Rust library for extracting data from XML documents, modeled on the F# library
[UseData.Xml](https://github.com/radekm/UseData.Xml).

**Status:** in design. See [PLAN.md](PLAN.md).

## Idea

The document is parsed into a tree of elements. You request attributes, children, and text,
and convert them with value parsers. Each piece of the document can be used only once.
At the end you call `check_all_used`, which fails if some attribute, text, or child
was not used, so changes in the input format are not silently ignored.

```rust
let root = use_xml::parse_str(xml)?;
let people = root
    .children("person")?
    .iter()
    .map(|p| Ok((p.attr("name", parse::string)?, p.attr_opt("address", parse::string)?)))
    .collect::<Result<Vec<_>, use_xml::Error>>()?;
root.check_all_used()?;
```

## Documentation

- [Design](docs/en/design.md)
- [Plan](PLAN.md)
- [Rules for contributors and agents](AGENTS.md)
