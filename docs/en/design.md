# Design

Czech version: [../cs/design.md](../cs/design.md).

`use-xml` extracts data from XML documents. It is a Rust counterpart of the F# library
[UseData.Xml](https://github.com/radekm/UseData.Xml). The API does not copy the original exactly:

- No closures. Navigation methods return child elements.
- Unused content is not reported when an element is dropped. The user explicitly calls
  `check_all_used`, which returns an error describing the first unused item.
- Attributes and children are stored in `BTreeMap`, so iteration order and error reporting are deterministic.
- Namespaces are supported. Elements and attributes are identified by a namespace and a local name.

## Data model

The whole document is parsed into a `Document`. It owns an arena of element nodes
(in document order, the root first) and the interned names:

```rust
pub struct Document {
    nodes: Vec<Node>,
    names: NameTable,
}

struct Node {
    name: QualifiedNameHandle,
    raw_name: u32,                 // Name as written (with the prefix), used in paths.
    parent: Option<u32>,           // Index of the parent node.
    index: u32,                    // 1-based index among siblings with the same qualified name.
    position: Position,            // Line and column of the start tag.
    text_significant: bool,
    all_used: Cell<bool>,          // `ignore_all` was called.
    // `None` after the text was used.
    text: Cell<Option<Box<str>>>,
    // `None` after the attribute was used or for a requested missing attribute.
    attrs: RefCell<BTreeMap<QualifiedNameHandle, Option<Box<str>>>>,
    // The flag says whether the name was used. Requested missing names have an empty `Vec`.
    children: RefCell<BTreeMap<QualifiedNameHandle, (bool, Vec<u32>)>>,
}

#[derive(Clone, Copy)]
pub struct Elem<'doc> {
    doc: &'doc Document,
    index: u32,
}

pub struct Position {
    pub line: u32,   // 1-based.
    pub column: u32, // 1-based, counted in Unicode scalar values.
}
```

`Elem<'doc>` is a small `Copy` handle. It borrows the document, so the document outlives
all its elements (an `Elem` which outlives its document does not compile). The parent
is an index into the arena, so a child can get its parent without reference cycles.

Attribute values and texts are moved out of the document when they are used, so their usage
does not need a separate flag: `Some` means unused, `None` means used. Values are stored
as `Box<str>`, which converts into `String` without copying. Elements are not moved;
the `bool` flag in `children` says whether a child name was used.

Requests for a missing attribute or child are recorded in the same maps:
a missing attribute gets `None`, a missing child name gets `(true, Vec::with_capacity(0))`.
Their qualified names are interned (see below) if they are not interned yet.
This is expected and not limited: the limits `MAX_ATTRS` and `MAX_CHILD_NAMES` apply only to the document,
so every request for a different missing name makes the document slightly bigger.

All API methods take `self` (`Elem` is `Copy`) or `&self` (`Document`). Usage is tracked
with interior mutability (`Cell`, `RefCell`), so a user can hold several elements
and still read the parent's attributes. A `RefCell` is never borrowed while user code
(e.g. a value parser) runs. `Document` is therefore `Send` but not `Sync`, and `Elem` is neither.

`Debug` of `Elem` prints only the element: its path, position, attributes, the number of children
for each name, and its text (used values are shown as `<used>`). Values longer than
`MAX_VALUE_CHARS = 100` characters are cut and end with `…`. `Debug` of `Document`
prints the number of elements and the root.

Document order is kept only among children with the same qualified name.
The order between children with different names is lost (as in the original).

### Interning

Namespace names (URIs) are interned into `NamespaceHandle(u8)` and qualified names
(a namespace handle and a local name) into `QualifiedNameHandle(u32)`.
A handle is an index into a `Vec`; a `BTreeMap` maps the name to its handle
(the strings are stored in both). The qualified names are kept in one map per namespace,
so a lookup by `(NamespaceHandle, &str)` does not allocate.

The empty namespace (`NamespaceHandle::NO_NAMESPACE`, handle 0) and the `xml` namespace
(`NamespaceHandle::XML`, handle 1) are always interned. Every declared namespace is interned
when its declaration is parsed.

After parsing, the names from the document are immutable, so e.g. `Elem::local_name` returns `&'doc str`.
Names interned later (`Document::namespace`, qualified names of requested missing items)
are kept in a second level in a `RefCell`; their handles continue after the handles of the parser.

A `NamespaceHandle` belongs to one document. Using a handle which is out of range
of the document is the error `InvalidNamespaceHandle`; a handle of another document
which happens to be in range cannot be detected.

Requested names are checked like the names in the document, so a request which can never
match is an error instead of a silently missing item:

- `Document::namespace` fails with `LimitExceeded` (`Limit::NamespaceLen`, without a position)
  for a namespace name longer than `MAX_NAMESPACE_LEN`, and with `InvalidNamespace`
  for a namespace name with a character which does not match `Char`
  or for `http://www.w3.org/2000/xmlns/` (it must not be declared). The empty namespace
  and the `xml` namespace are valid. Namespace names are not checked to be URIs
  (the parser does not check it either).
- `attr`, `attr_opt`, `child`, `child_opt`, and `children` fail with `InvalidName`
  if the local name is not an `NCName` (e.g. it is empty or contains a colon)
  or is longer than `MAX_NAME_LEN`. Invalid names are not interned or marked as used.

### Paths

The path of an element is formatted like XPath from the names as written in the document
(with prefixes): the root without an index, other elements with a 1-based index among siblings
with the same qualified name, e.g. `/root/items[1]/item[3]` or `/x/p:item[2]`.
Paths are built only when needed (errors, `Elem::path`) by walking the parents.

## Parsing rules

The parser is our own (no `quick-xml`). It accepts only documents which are well-formed
XML 1.0 and namespace-well-formed (Namespaces in XML 1.0) and which follow the rules below.
Any violation is a parse error, not an error discovered later during extraction.
The parser uses no recursion; open elements are kept on an explicit stack.

### Characters and names

- Every character must match the `Char` production of XML 1.0 (no control characters except
  tab, line feed, and carriage return, no `U+FFFE` and `U+FFFF`), also when written
  as a character reference (`&#1;` is an error).
- Element names, attribute names, processing instruction targets, and entity names must match
  the `Name` production. With namespaces, element and attribute names are `QName`s
  (`prefix:local` or `local`, at most one colon, neither part empty);
  processing instruction targets and entity names contain no colon.
- Whitespace means XML whitespace: space, tab, carriage return, and line feed.
- Line endings are normalized: `\r\n` and a lone `\r` become `\n`.

### Limits

If a limit is exceeded, parsing fails with `LimitExceeded`:

- `MAX_NAME_LEN = 512` bytes (not code points) of a local name of an element or an attribute,
  of a processing instruction target, and of an entity name.
- `MAX_PREFIX_LEN = 64` bytes of a namespace prefix (also in `xmlns:prefix`).
- `MAX_NAMESPACE_LEN = 4096` bytes of a namespace name (after decoding references).
- `MAX_NAMESPACES = 256` different namespaces in a document (including the empty and the `xml` namespace).
  `Document::namespace` fails with the same error (without a position), and also for a namespace name
  longer than `MAX_NAMESPACE_LEN`.
- `u32::MAX + 1` qualified names (the handles are all values of `u32`).
- `MAX_ATTRS = 1000` attributes of one element, namespace declarations included.
- `MAX_CHILD_NAMES = 1000` different qualified names of children of one element.
  Children with the same qualified name can repeat any number of times.
- `u32::MAX + 1` elements in a document.
- `MAX_DEPTH = 256` nesting levels (error `MaxDepthExceeded`). The root element has depth 1.

### Namespaces

- `xmlns="uri"` declares the default namespace, `xmlns=""` undeclares it,
  `xmlns:p="uri"` binds the prefix `p`. A declaration is in scope in its element
  (including the element's own name and attributes, even if they precede the declaration)
  and in its descendants.
- An unprefixed element is in the default namespace (or in no namespace).
  An unprefixed attribute is always in no namespace.
- The prefix `xml` is bound to `http://www.w3.org/XML/1998/namespace`. It may be declared only
  with this namespace, and this namespace must not be bound to another prefix or be the default namespace.
- The prefix `xmlns` must not be declared or used for elements; the namespace
  `http://www.w3.org/2000/xmlns/` must not be declared.
- A prefix must not be bound to the empty namespace (`xmlns:p=""`).
- A prefix which is not declared is an error.
- Namespace declarations are not attributes: they cannot be requested and are not reported as unused.
- These are `Namespace` errors, except for limits.
- Two attributes of one element with the same name as written, or with the same namespace and local name
  (e.g. `a:x` and `b:x` when `a` and `b` are bound to the same namespace), are a `DuplicateAttr` error.
  This includes duplicate namespace declarations. `xml:lang` and `lang` are different attributes.
- Namespace errors in a start tag (e.g. an undeclared prefix) are reported before duplicate attributes,
  because the path in `DuplicateAttr` needs the namespace of the element. So in `<a x='1' x='2' p:y='3'/>`
  the undeclared prefix `p` is reported, not the duplicate `x`.

### Content

- Mixed content is an error: an element either has child elements or text, not both.
  Whitespace between child elements is allowed and ignored.
- If the element has child elements, its text is `""` and not significant.
- If the element has no child elements, its text is the concatenation of all its text and CDATA,
  including whitespace. The text is significant when it contains a non-whitespace character
  or at least one CDATA section (even an empty one). A character reference to a whitespace
  character is whitespace.
- An empty element (`<a/>` or `<a></a>`) has text `""` that is not significant.
- Only the predefined entities (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`)
  and character references (`&#N;`, `&#xH;`) are supported. Other entities are an `UnknownEntity` error.
  Errors in references point to `&`.
- `]]>` is not allowed in text, `<` is not allowed in attribute values.
- Attribute values are normalized: literal tab, line feed, and carriage return
  (`\r\n` as one) become spaces; characters from references are kept.
- Whitespace is required between attributes. Whitespace around `=` and before `>` of an end tag is allowed.
- Comments must not contain `--`. Comments and processing instructions are ignored everywhere.
  A processing instruction target must not be `xml` in any case.

### Document

- `parse_str(&str)` parses an already decoded document; the encoding in the XML declaration is only checked
  syntactically.
- `parse_bytes(&[u8])` parses a document encoded in UTF-8. Other encodings are not supported:
  UTF-16 or UTF-32 (detected by a byte order mark or by the encoded `<` at the start),
  an encoding other than UTF-8 in the XML declaration (case-insensitive),
  or invalid UTF-8 result in an `Encoding` error.
- A single leading byte order mark is skipped and not counted in positions. A second one is an error.
- The XML declaration is allowed only at the very start. It must contain `version` with a value `1.x`,
  and optionally `encoding` (a valid encoding name) and `standalone` (`yes` or `no`), in this order.
  An XML declaration elsewhere is an `UnexpectedNode` error.
- Before the root element only the XML declaration, whitespace, comments, and processing instructions
  are allowed. Anything else (`DOCTYPE`, text, references, CDATA) is an `UnexpectedNode` error.
- After the root element only whitespace, comments, and processing instructions are allowed.
- Malformed XML (unclosed or mismatched tags, invalid syntax) is an `Xml` error.

## Usage tracking

Each of the following counts as one use:

- an attribute (requested via `attr` / `attr_opt`),
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

`elem.check_all_used()` (or `doc.check_all_used()` for the root) walks the subtree depth-first
and returns an error for the first unused item:

1. an unused attribute,
2. unused significant text (whitespace-only text that was never requested is not reported),
3. an unused child name; the error has the path and the position
   of the first child with this name, not of the parent,
4. otherwise, it continues with the children of used names (in document order within a name).

Attributes and child names are ordered by their qualified name handles, i.e. by the first occurrence
of the qualified name in the document. Elements marked by `ignore_all` are skipped.
`check_all_used` and `ignore_all` use no recursion.

## API sketch

```rust
let doc = use_xml::parse_str(xml)?;            // Result<Document, Error>
let doc = use_xml::parse_bytes(bytes)?;        // Result<Document, Error>, UTF-8 only

let ns = doc.namespace("urn:x")?;              // NamespaceHandle; interned if missing
let no = doc.namespace("")?;                   // = NamespaceHandle::NO_NAMESPACE
doc.namespace_uri(ns);                         // Option<Cow<str>>, borrowed for namespaces from the document
let root = doc.root();                         // Elem<'_>

root.local_name();                             // &str
root.namespace();                              // NamespaceHandle
root.namespace_uri();                          // &str
root.prefixed_name();                          // &str – as written, e.g. `p:item`
root.parent();                                 // Option<Elem>
root.document();                               // &Document
root.path();                                   // String, e.g. `/root/items[1]/item[3]`
root.position();                               // Position

root.attr(no, "id", parse::u32)?;              // T, error if missing
root.attr_opt(no, "note", parse::string)?;     // Option<T>
root.text(parse::decimal)?;                    // T

root.child(ns, "pig")?;                        // Elem, error unless exactly one
root.child_opt(ns, "cat")?;                    // Option<Elem>, error if more than one
root.children(ns, "dog")?;                     // Vec<Elem>

root.ignore_all();
root.check_all_used()?;
doc.check_all_used()?;
```

A value parser is any function `FnOnce(String) -> Result<T, E>` where `E: Display`.
The value is moved into the parser, so `parse::string` returns it without copying;
`str::parse` can be used as `|s: String| s.parse::<u8>()`.
The parsers in the `parse` module return `Result<T, String>`; their messages contain the rejected value
(cut to `MAX_VALUE_CHARS` characters), e.g. `expected u8, got "x"`.
The library wraps the error with context (element path, position, attribute name or "text").
The `parse` module provides ready-made parsers, ported from the original:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
integer types (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool`, `date_time_utc`, and `date_time_offset` (both `time::OffsetDateTime`).
Helpers `map` / `validate` combine parsers.

Value formats (all strict, no surrounding whitespace):

- Integers: Rust `from_str` – digits with an optional leading `+` or `-`.
- `decimal`: an optional leading `+` or `-`, digits, an optional `.` followed by digits.
  No exponent, no thousands separators. The scale is the number of digits after `.`
  (`12.50` stays `12.50`). If the number cannot be represented with this scale
  (`Decimal` allows at most 28 digits after `.`), trailing zeros after `.` are removed.
  Numbers which still cannot be represented exactly are rejected.
- `bool`: exactly `true` or `false`.
- `date_time_utc`: `YYYY-MM-DDTHH:MM:SS[.fffffffff]Z`; a space may be used instead of `T`;
  the fraction has 1–9 digits; only `Z` is accepted as the offset.
- `date_time_offset`: the same, but the offset is `Z` or `+HH:MM` / `-HH:MM`.

## Errors

The library has one error type `Error` (made with `thiserror`). It is a box around `ErrorKind`,
so `Result<T, Error>` stays small; the details are available via `Error::kind` and `Error::into_kind`,
and `Error` can be compared with `ErrorKind` directly. Variants of `ErrorKind`:

- `Xml` – malformed XML, with position.
- `Namespace` – the document is not namespace-well-formed (undeclared prefix, reserved prefix or namespace,
  invalid qualified name).
- `Encoding` – invalid UTF-8 or an unsupported encoding.
- `UnexpectedNode` – e.g. `DOCTYPE`, a second root element, text outside the root element,
  or an XML declaration which is not at the start.
- `LimitExceeded` – a limit from above (`Limit`), with an optional position
  (`None` only from `Document::namespace`).
- `UnknownEntity` – an entity reference which is neither predefined nor a character reference;
  the position points to `&`.
- `MixedContent` – the path of the element and the position of its significant text
  (the first non-whitespace character, the start of the CDATA section, or `&` of the reference).
- `DuplicateAttr` – the path of the element, the position and the name (as written) of the second attribute.
- `MaxDepthExceeded` – path of the deepest allowed element, position of the offending child.
- `InvalidNamespaceHandle` – a namespace handle which does not belong to the document.
- `InvalidNamespace` – `Document::namespace` received a namespace name which cannot occur in a document.
- `InvalidName` – `attr`, `child`, … received a local name which is not an `NCName` or is too long.
- `Missing` – a required attribute or child (`attr`, `child`) is missing.
- `WrongCount` – `child` or `child_opt` found more than one child.
- `AlreadyUsed`
- `InvalidValue` – a value parser failed; contains the attribute (or text) and the message.
- `Unused` – reported by `check_all_used`.

`Error::position()` (and `ErrorKind::position()`) returns `Option<Position>`; it is `None` only
for `LimitExceeded` from `Document::namespace` and for `InvalidNamespace`.
`Error::path()` (and `ErrorKind::path()`) returns the path of the related element,
or `None` for `Xml`, `Namespace`, `Encoding`, `UnexpectedNode`, `LimitExceeded`, `UnknownEntity`,
and `InvalidNamespace`.

`AlreadyUsed`, `Missing`, `InvalidValue`, `Unused`, and `InvalidName` contain an `Item`:
`Item::Attr(QualifiedName)`, `Item::Child(QualifiedName)`, or `Item::Text`.
`QualifiedName` contains the namespace name and the local name and is displayed as `local`
for no namespace and as `{namespace}local` otherwise.
