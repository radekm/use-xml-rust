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
    path: Arc<ElemPath>,                      // For error messages, e.g. `/root/items[1]/item[3]`.
    position: Position,                       // Line and column of the start tag.
    // Key is the local name; the flag says whether the attribute was used.
    attrs: BTreeMap<String, (Cell<bool>, String)>,
    // Requested names of attributes which do not exist.
    missing_attrs_used: RefCell<BTreeSet<String>>,
    // Key is the local name; document order within a name.
    children: BTreeMap<String, (Cell<bool>, Vec<Elem>)>,
    // Requested names of children which do not exist.
    missing_children_used: RefCell<BTreeSet<String>>,
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

`ElemPath` is a node of a linked list `{ parent: Option<Arc<ElemPath>>, name, prefix, index }`
shared between an element and its descendants, so each element costs only one small allocation.
The path is formatted like XPath: the root without an index, other elements with a 1-based index
among siblings with the same local name, e.g. `/root/items[1]/item[3]`.

The "used" flag of an existing attribute or child name is stored next to its value,
so using it does not allocate. Only requests for names which do not exist
are recorded in the `missing_*_used` sets (so that a second request fails too).

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
- Whitespace means XML whitespace: space, tab, carriage return, and line feed.
- Line endings in text and CDATA are normalized to `\n`.
  Attribute values are normalized as required by XML (e.g. line breaks become spaces).
- Known limitation: some well-formedness constraints which `quick-xml` does not check
  (e.g. forbidden control characters or `]]>` in text) are not checked either.

### Document

- `parse_str(&str)` parses an already decoded document; the encoding in the XML declaration is ignored.
- `parse_bytes(&[u8])` parses a document encoded in UTF-8. Other encodings are not supported:
  UTF-16 or UTF-32 (detected by a byte order mark or by the encoded `<` at the start),
  an encoding other than UTF-8 in the XML declaration (case-insensitive),
  or invalid UTF-8 result in an `Encoding` error.
- A single leading byte order mark is skipped (by `quick-xml`). A second one is an error.

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
let root = use_xml::parse_bytes(bytes)?;       // Result<Elem, Error>, UTF-8 only

root.name();                                   // &str – local name
root.position();                               // Position

root.attr("id", parse::u32)?;                  // T, error if missing
root.attr_opt("note", parse::string)?;         // Option<T>
root.text(parse::decimal)?;                    // T

root.path();                                   // String, e.g. `/root/items[1]/item[3]`
root.child("pig")?;                            // &Elem, error unless exactly one
root.child_opt("cat")?;                        // Option<&Elem>, error if more than one
root.children("dog")?;                         // &[Elem]

root.ignore_all();
root.check_all_used()?;
```

A value parser is any function `FnOnce(&str) -> Result<T, E>` where `E: Display`
(so e.g. `str::parse::<u8>` works too). The parsers in the `parse` module return `Result<T, String>`. The library wraps its error
with context (element name, position, attribute name or "text", input value).
The `parse` module provides ready-made parsers, ported from the original:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
integer types (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool`, `date_time_utc`, and `date_time_offset` (both `time::OffsetDateTime`).
Helpers `map` / `validate` combine parsers.

Value formats (all strict, no surrounding whitespace):

- Integers: Rust `from_str` – digits with an optional leading `+` or `-`.
- `decimal`: an optional leading `+` or `-`, digits, an optional `.` followed by digits.
  No exponent, no thousands separators.
- `bool`: exactly `true` or `false`.
- `date_time_utc`: `YYYY-MM-DDTHH:MM:SS[.fffffffff]Z`; a space may be used instead of `T`;
  the fraction has 1–9 digits; only `Z` is accepted as the offset.
- `date_time_offset`: the same, but the offset is `Z` or `+HH:MM` / `-HH:MM`.

## Errors

The library has one error type `Error` (made with `thiserror`). Every error related
to an element carries the path and position of the element. Variants:

- `Xml` – malformed XML (from `quick-xml`), with position.
- `Encoding` – invalid UTF-8 or an unsupported encoding (only from `parse_bytes`).
- `UnexpectedNode` – e.g. `DOCTYPE`, a second root element, or text outside the root element.
- `UnknownEntity` – an entity reference which is neither predefined nor a character reference.
- `MixedContent`
- `DuplicateAttr` – attributes with the same local name.
- `ChildPrefixMismatch` – children with the same local name but different prefixes
  (path of the parent, position of the offending child).
- `MaxDepthExceeded` – path of the deepest allowed element, position of the offending child.
- `Missing` – a required attribute or child (`attr`, `child`) is missing.
- `WrongCount` – `child` or `child_opt` found more than one child.
- `AlreadyUsed`
- `InvalidValue` – a value parser failed; contains the attribute name (or text), the input, and the message.
- `Unused` – reported by `check_all_used`.

`AlreadyUsed`, `Missing`, `InvalidValue`, and `Unused` contain an `Item`:
`Item::Attr(name)`, `Item::Child(name)`, or `Item::Text`.
