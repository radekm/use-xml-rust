# Design

Czech version: [../cs/design.md](../cs/design.md).

`use-xml` extracts data from XML documents. It is a Rust counterpart of the F# library
[UseData.Xml](https://github.com/radekm/UseData.Xml). The API does not copy the original exactly:

- No closures. Navigation methods return references to child elements.
- Unused content is not reported when an element is dropped. The user explicitly calls
  `check_all_used`, which returns an error describing the first unused item.
- Attributes and children are stored in `BTreeMap`, so iteration order and error reporting are deterministic.

## Data model

The whole document is parsed into a tree of `Elem` values:

```rust
pub struct Elem {
    local_name: String,
    position: Position,                       // Line and column of the start tag.
    attrs: BTreeMap<String, String>,          // Key is the local name.
    attrs_used: RefCell<BTreeSet<String>>,
    children: BTreeMap<String, Vec<Elem>>,    // Key is the local name; document order within a name.
    children_used: RefCell<BTreeSet<String>>,
    text: String,
    // The text contains a non-whitespace character or a CDATA section.
    text_significant: bool,
    text_used: Cell<bool>,
    // `ignore_all` was called: everything in the element, including its descendants, is used
    // and any further request fails with "already used".
    all_used: Cell<bool>,
}

pub struct Position {
    pub line: u32,   // 1-based.
    pub column: u32, // 1-based, counted in Unicode scalar values.
}
```

All API methods take `&self`. Usage is tracked with interior mutability (`Cell`, `RefCell`),
so a user can hold references to several children and still read the parent's attributes.
`Elem` is therefore not `Sync`.

Document order is kept only among children with the same local name.
The order between children with different names is lost (as in the original).

## Parsing rules

Parsing builds the whole tree and checks all structural rules up front.
Any violation is a parse error, not an error discovered later during extraction.

### Names and namespaces

- Namespaces are ignored. Elements and attributes are identified only by their local name
  (the part after the prefix).
- Attributes `xmlns` and `xmlns:*` are skipped.
- Two attributes of one element with the same local name (e.g. `a:x` and `b:x`) are an error.
- Two children of one element with the same local name but different prefixes
  (e.g. `<a:item/>` and `<b:item/>`) are an error, so one `Vec<Elem>` never mixes elements
  with different qualified names. Children with the same prefix (or both without a prefix) are fine.
  Note: the default namespace (`xmlns="..."`) does not change the prefix, so it is not detected.

### Content

- Mixed content is an error: an element either has child elements or text, not both.
  Whitespace between child elements is allowed and ignored.
- If the element has child elements, its text is `""` and not significant.
- If the element has no child elements, its text is the concatenation of all its text and CDATA,
  including whitespace. The text is significant when it contains a non-whitespace character
  or at least one CDATA section (even an empty one).
- An empty element (`<a/>` or `<a></a>`) has text `""` that is not significant.
- Entity and character references are decoded. Only the predefined XML entities
  (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`) and character references are supported.
- Comments and processing instructions are ignored everywhere.

### Document

- Before the root element only the XML declaration, whitespace, comments, and processing instructions
  are allowed. Anything else (including `DOCTYPE`) is an error.
- After the root element only whitespace, comments, and processing instructions are allowed.
- Malformed XML (unclosed or mismatched tags, invalid syntax) is an error.
- The nesting depth is limited by `MAX_DEPTH = 256`. The root element has depth 1.
  An element at depth 257 is an error. Parsing does not use recursion, so deep documents
  cannot overflow the stack.

## Usage tracking

Each of the following counts as one use:

- an attribute name (requested via `attr` / `attr_opt`),
- a child name (requested via `child` / `child_opt` / `children`),
- the text (requested via `text`).

Rules:

- Each item can be used at most once. A second request fails with an "already used" error.
- Requesting a missing attribute or child also marks the name as used,
  so a second request for it fails as well.
- Requesting a child name marks all children with that name as used.
  The user always gets all of them (`children`); `child` and `child_opt` additionally check the count.
- `ignore_all` marks the element and all its descendants as used. Any later request
  on the element (or its descendants) fails with "already used".

### `check_all_used`

`elem.check_all_used()` walks the subtree depth-first and returns an error for the first unused item:

1. an unused attribute (in name order),
2. unused significant text (whitespace-only text that was never requested is not reported),
3. an unused child name (in name order),
4. otherwise, it recurses into the children of used names (in name order, then document order).

Elements marked by `ignore_all` are skipped.

## API sketch

```rust
let root = use_xml::parse_str(xml)?;           // Result<Elem, Error>

root.name();                                   // &str – local name
root.position();                               // Position

root.attr("id", parse::u32)?;                  // T, error if missing
root.attr_opt("note", parse::string)?;         // Option<T>
root.text(parse::decimal)?;                    // T

root.child("pig")?;                            // &Elem, error unless exactly one
root.child_opt("cat")?;                        // Option<&Elem>, error if more than one
root.children("dog")?;                         // &[Elem]

root.ignore_all();
root.check_all_used()?;
```

A value parser is a function `Fn(&str) -> Result<T, String>`. The library wraps its error
with context (element name, position, attribute name or "text", input value).
The `parse` module provides ready-made parsers, ported from the original:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
integer types (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool`, and `date_time_utc` (`time::OffsetDateTime`).
Helpers `map` / `validate` combine parsers.

## Errors

The library has one error type `Error` (made with `thiserror`). Every error carries
the local name and position of the element it relates to. Planned variants:

- `Xml` – malformed XML (from `quick-xml`), with position.
- `UnexpectedNode` – e.g. `DOCTYPE`, or text outside the root element.
- `MixedContent`
- `DuplicateAttr` – attributes with the same local name.
- `ChildPrefixMismatch` – children with the same local name but different prefixes.
- `MaxDepthExceeded`
- `Missing` – a required attribute or child is missing.
- `WrongCount` – `child` / `child_opt` found an unexpected number of children.
- `AlreadyUsed`
- `InvalidValue` – a value parser failed; contains the attribute name (or text), the input, and the message.
- `Unused` – reported by `check_all_used`.
