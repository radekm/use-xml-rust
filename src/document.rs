//! Parsing of an XML document into a tree of [`Elem`].

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;

use quick_xml::XmlVersion;
use quick_xml::escape::{EscapeError, resolve_predefined_entity};
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::elem::{Elem, ElemPath};
use crate::error::{Error, ErrorKind, Position};

/// Maximum nesting depth of elements. The root element has depth 1.
pub const MAX_DEPTH: usize = 256;

/// Whitespace as defined by the XML specification.
fn is_xml_whitespace(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Converts byte offsets into lines and columns.
///
/// Offsets are usually requested in increasing order, so the tracker remembers
/// the last position and continues from it.
struct PositionTracker<'a> {
    input: &'a str,
    offset: usize,
    position: Position,
}

impl<'a> PositionTracker<'a> {
    /// `input` is the whole document. A leading byte order mark is skipped
    /// because `quick-xml` removes it and reports offsets relative to the rest of the input.
    fn new(input: &'a str) -> Self {
        Self {
            input: input.strip_prefix('\u{FEFF}').unwrap_or(input),
            offset: 0,
            position: Position { line: 1, column: 1 },
        }
    }

    fn position_at(&mut self, offset: u64) -> Position {
        let mut offset = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.input.len());
        while !self.input.is_char_boundary(offset) {
            offset -= 1;
        }
        if offset < self.offset {
            self.offset = 0;
            self.position = Position { line: 1, column: 1 };
        }
        let bytes = self.input.as_bytes();
        for i in self.offset..offset {
            let b = bytes[i];
            // `\r\n` and a lone `\r` are line breaks (only `\n` of `\r\n` advances the line).
            let line_break = b == b'\n' || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n'));
            if line_break {
                self.position.line = self.position.line.saturating_add(1);
                self.position.column = 1;
            } else if b != b'\r' && (b & 0xC0) != 0x80 {
                // Count only the first byte of each UTF-8 encoded character.
                self.position.column = self.position.column.saturating_add(1);
            }
        }
        self.offset = offset;
        self.position
    }
}

/// An element whose end tag was not reached yet.
struct Builder {
    path: Arc<ElemPath>,
    position: Position,
    attrs: BTreeMap<String, (Cell<bool>, String)>,
    /// A child is added when it ends. The builder receives events only while it is
    /// the innermost open element, i.e. when all its children have already ended,
    /// so `children` is non-empty iff the element has children.
    children: BTreeMap<String, (Cell<bool>, Vec<Elem>)>,
    text: String,
    text_significant: bool,
}

impl Builder {
    fn finish(self) -> Elem {
        let text = if self.children.is_empty() {
            self.text.into_boxed_str()
        } else {
            Box::default()
        };
        Elem {
            path: self.path,
            position: self.position,
            attrs: self.attrs,
            children: self.children,
            missing_used: RefCell::new(None),
            text,
            text_significant: self.text_significant,
            text_used: Cell::new(false),
            all_used: Cell::new(false),
        }
    }

    fn mixed_content(&self) -> Error {
        ErrorKind::MixedContent {
            path: self.path.to_string(),
            position: self.position,
        }
        .into()
    }

    /// Adds text. `significant` is true for CDATA and text with a non-whitespace character.
    fn add_text(&mut self, text: &str, significant: bool) -> Result<(), Error> {
        if significant {
            if !self.children.is_empty() {
                return Err(self.mixed_content());
            }
            self.text_significant = true;
            self.text.push_str(text);
        } else if self.children.is_empty() {
            // Whitespace is kept only if the element has no children.
            self.text.push_str(text);
        }
        Ok(())
    }
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    tracker: PositionTracker<'a>,
    stack: Vec<Builder>,
    root: Option<Elem>,
    /// Whether the encoding in the XML declaration must be UTF-8.
    check_encoding: bool,
}

/// Parses an XML document into a tree of elements and returns the root element.
///
/// All structural rules (see the design documentation) are checked here:
/// mixed content, duplicate attributes, children with different prefixes,
/// `DOCTYPE`, unknown entities, and [`MAX_DEPTH`].
///
/// A leading byte order mark is skipped. The encoding in the XML declaration is ignored
/// because the input is already decoded.
///
/// ```
/// let root = use_xml::parse_str("<animals><dog>A</dog><dog>D</dog></animals>")?;
/// assert_eq!(root.name(), "animals");
/// assert_eq!(root.children("dog")?.len(), 2);
/// # Ok::<(), use_xml::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if the document is not well-formed or violates a structural rule.
pub fn parse_str(input: &str) -> Result<Elem, Error> {
    parse(input, false)
}

/// Parses an XML document encoded in UTF-8 into a tree of elements and returns the root element.
///
/// A leading UTF-8 byte order mark is skipped. Other encodings are not supported:
/// UTF-16 or UTF-32 (detected by a byte order mark or by the encoded `<` at the start),
/// an encoding other than UTF-8 in the XML declaration,
/// or invalid UTF-8 result in [`ErrorKind::Encoding`]. Otherwise it behaves like [`parse_str`].
///
/// ```
/// let root = use_xml::parse_bytes(b"<?xml version='1.0' encoding='UTF-8'?><a x='1'/>")?;
/// assert_eq!(root.attr("x", use_xml::parse::u8)?, 1);
/// # Ok::<(), use_xml::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if the input is not UTF-8, the document is not well-formed,
/// or it violates a structural rule.
pub fn parse_bytes(input: &[u8]) -> Result<Elem, Error> {
    if let Some(encoding) = detect_unsupported_encoding(input) {
        return Err(ErrorKind::Encoding {
            message: format!("{encoding} is not supported"),
            position: Position { line: 1, column: 1 },
        }
        .into());
    }
    // The UTF-8 byte order mark is valid UTF-8, it is skipped later by `quick-xml`.
    let input = std::str::from_utf8(input).map_err(|e| -> Error {
        // The prefix up to `valid_up_to` is valid UTF-8.
        let valid = std::str::from_utf8(&input[..e.valid_up_to()]).unwrap_or_default();
        ErrorKind::Encoding {
            message: "invalid UTF-8".to_owned(),
            position: PositionTracker::new(valid).position_at(valid.len() as u64),
        }
        .into()
    })?;
    parse(input, true)
}

/// Detects UTF-16 and UTF-32 by a byte order mark or, if there is none,
/// by the encoded `<` at the start of the document (XML 1.0, appendix F).
fn detect_unsupported_encoding(input: &[u8]) -> Option<&'static str> {
    // UTF-32 must be checked first because its byte order marks start like UTF-16 ones.
    const PREFIXES: [(&[u8], &str); 8] = [
        (b"\x00\x00\xFE\xFF", "UTF-32"),
        (b"\xFF\xFE\x00\x00", "UTF-32"),
        (b"\x00\x00\x00<", "UTF-32"),
        (b"<\x00\x00\x00", "UTF-32"),
        (b"\xFE\xFF", "UTF-16"),
        (b"\xFF\xFE", "UTF-16"),
        (b"\x00<", "UTF-16"),
        (b"<\x00", "UTF-16"),
    ];
    PREFIXES
        .iter()
        .find(|(prefix, _)| input.starts_with(prefix))
        .map(|(_, encoding)| *encoding)
}

/// Returns the byte index of the first reference in the raw attribute value `raw`
/// which is not closed by `;` or cannot be resolved.
fn first_invalid_reference(raw: &str) -> Option<usize> {
    raw.match_indices('&').map(|(i, _)| i).find(|&i| {
        match raw[i..].find(';') {
            None => true,
            // `i` and `i + end` are indices of ASCII characters, so the slice is valid.
            Some(end) => quick_xml::escape::unescape(&raw[i..=i + end]).is_err(),
        }
    })
}

/// Parses `input`. A leading byte order mark is removed by `quick-xml`.
fn parse(input: &str, check_encoding: bool) -> Result<Elem, Error> {
    let mut reader = Reader::from_str(input);
    let config = reader.config_mut();
    config.enable_all_checks(true);
    config.expand_empty_elements = false;
    config.trim_text(false);

    let mut parser = Parser {
        reader,
        tracker: PositionTracker::new(input),
        stack: Vec::new(),
        root: None,
        check_encoding,
    };
    parser.run()
}

impl<'a> Parser<'a> {
    fn xml_error(&mut self, message: String, offset: u64) -> Error {
        ErrorKind::Xml {
            message,
            position: self.tracker.position_at(offset),
        }
        .into()
    }

    /// Converts an error in an attribute list. `content_offset` is the offset of the content
    /// to which positions in `AttrError` are relative: the byte after `<` for a start tag
    /// and the byte after `<?` for the XML declaration.
    fn attr_error(&mut self, error: AttrError, content_offset: u64) -> Error {
        let (relative, message) = match error {
            AttrError::ExpectedEq(pos) => (
                pos,
                "attribute name must be directly followed by `=` or whitespace".to_owned(),
            ),
            AttrError::ExpectedValue(pos) => {
                (pos, "`=` must be followed by an attribute value".to_owned())
            }
            AttrError::UnquotedValue(pos) => (
                pos,
                "attribute value must be enclosed in `\"` or `'`".to_owned(),
            ),
            AttrError::ExpectedQuote(pos, quote) => (
                pos,
                format!(
                    "missing closing quote `{}` in attribute value",
                    char::from(quote)
                ),
            ),
            AttrError::Duplicated(pos, _) => (pos, "duplicated attribute".to_owned()),
        };
        self.xml_error(message, content_offset + relative as u64)
    }

    /// Returns the offset of `value` in the input if `value` is a part of the input.
    ///
    /// `quick-xml` reading from a `&str` returns attribute values which borrow the input,
    /// so their offset can be computed from their address.
    fn offset_in_input(&self, value: &str) -> Option<u64> {
        let input = self.tracker.input;
        let offset = value.as_ptr().addr().checked_sub(input.as_ptr().addr())?;
        (offset + value.len() <= input.len()).then_some(offset as u64)
    }

    /// Converts an error of the normalization of the attribute value `raw`.
    ///
    /// The error points to the first invalid reference in `raw`. If it cannot be found,
    /// the error points to the element at `element_position`.
    fn attr_value_error(
        &mut self,
        error: quick_xml::Error,
        raw: &str,
        path: &ElemPath,
        element_position: Position,
    ) -> Error {
        let position = self
            .offset_in_input(raw)
            .and_then(|value_offset| first_invalid_reference(raw).map(|i| value_offset + i as u64))
            .map_or(element_position, |offset| self.tracker.position_at(offset));
        match error {
            quick_xml::Error::Escape(EscapeError::UnrecognizedEntity(_, name)) => {
                ErrorKind::UnknownEntity {
                    name,
                    path: path.to_string(),
                    position,
                }
            }
            // The message of `quick-xml` contains a byte range in the value, which is confusing
            // next to the position in the document.
            quick_xml::Error::Escape(EscapeError::UnterminatedEntity(_)) => ErrorKind::Xml {
                message: "entity or character reference not closed: `;` not found".to_owned(),
                position,
            },
            e => ErrorKind::Xml {
                message: e.to_string(),
                position,
            },
        }
        .into()
    }

    fn unexpected(&mut self, node: &str, offset: u64) -> Error {
        ErrorKind::UnexpectedNode {
            node: node.to_owned(),
            position: self.tracker.position_at(offset),
        }
        .into()
    }

    fn run(&mut self) -> Result<Elem, Error> {
        let mut first_event = true;
        loop {
            let offset = self.reader.buffer_position();
            let event = match self.reader.read_event() {
                Ok(event) => event,
                Err(e) => {
                    let offset = self.reader.error_position();
                    return Err(self.xml_error(e.to_string(), offset));
                }
            };
            let is_first = std::mem::replace(&mut first_event, false);
            match event {
                Event::Start(start) => {
                    let builder = self.start_element(&start, offset)?;
                    self.stack.push(builder);
                }
                Event::Empty(start) => {
                    let builder = self.start_element(&start, offset)?;
                    self.end_element(builder);
                }
                Event::End(_) => {
                    // `check_end_names` guarantees that the end tag matches an open element.
                    match self.stack.pop() {
                        Some(builder) => self.end_element(builder),
                        None => return Err(self.unexpected("end tag", offset)),
                    }
                }
                Event::Text(text) => {
                    // Errors point to the first non-whitespace character, not to the leading whitespace.
                    let skipped = text
                        .as_ref()
                        .bytes()
                        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                        .unwrap_or(0);
                    let text = text.xml10_content();
                    self.add_text(&text, false, offset + skipped as u64, "text")?;
                }
                Event::CData(cdata) => {
                    let text = cdata.xml10_content();
                    self.add_text(&text, true, offset, "CDATA section")?;
                }
                Event::GeneralRef(reference) => {
                    // References are not allowed outside of the root element,
                    // not even references to whitespace characters.
                    let Some(builder) = self.stack.last() else {
                        return Err(
                            self.unexpected("entity reference outside of the root element", offset)
                        );
                    };
                    let mut buffer = [0; 4];
                    let resolved: &str = match reference.resolve_char_ref() {
                        Ok(Some(c)) => c.encode_utf8(&mut buffer),
                        Ok(None) => match resolve_predefined_entity(&reference) {
                            Some(s) => s,
                            None => {
                                return Err(ErrorKind::UnknownEntity {
                                    name: reference.to_string(),
                                    path: builder.path.to_string(),
                                    position: self.tracker.position_at(offset),
                                }
                                .into());
                            }
                        },
                        Err(e) => return Err(self.xml_error(e.to_string(), offset)),
                    };
                    self.add_text(resolved, false, offset, "entity reference")?;
                }
                Event::Decl(decl) => {
                    if !is_first {
                        return Err(self.unexpected("XML declaration", offset));
                    }
                    if self.check_encoding {
                        match decl.encoding() {
                            None => {}
                            Some(Ok(encoding)) if encoding.eq_ignore_ascii_case("utf-8") => {}
                            Some(Ok(encoding)) => {
                                return Err(ErrorKind::Encoding {
                                    message: format!("encoding `{encoding}` is not supported"),
                                    position: self.tracker.position_at(offset),
                                }
                                .into());
                            }
                            // Positions in `AttrError` are relative to the byte after `<?`.
                            Some(Err(e)) => return Err(self.attr_error(e, offset + 2)),
                        }
                    }
                }
                Event::DocType(_) => return Err(self.unexpected("DOCTYPE", offset)),
                Event::Comment(_) | Event::PI(_) => {}
                Event::Eof => {
                    if !self.stack.is_empty() {
                        return Err(self.xml_error("unexpected end of document".to_owned(), offset));
                    }
                    return match self.root.take() {
                        Some(root) => Ok(root),
                        None => Err(self.xml_error("missing root element".to_owned(), offset)),
                    };
                }
            }
        }
    }

    /// Adds text to the innermost open element.
    ///
    /// For text and entity references `significant` is computed from the content.
    fn add_text(&mut self, text: &str, cdata: bool, offset: u64, node: &str) -> Result<(), Error> {
        let significant = cdata || !is_xml_whitespace(text);
        match self.stack.last_mut() {
            Some(builder) => builder.add_text(text, significant),
            None if significant => {
                Err(self.unexpected(&format!("{node} outside of the root element"), offset))
            }
            None => Ok(()),
        }
    }

    fn start_element(&mut self, start: &BytesStart<'_>, offset: u64) -> Result<Builder, Error> {
        let position = self.tracker.position_at(offset);
        let qname = start.name();
        let (local_name, prefix) = qname.decompose();
        let local_name: &str = local_name.as_ref();
        let prefix: &str = prefix.as_ref().map_or("", |p| p.as_ref());

        let depth = self.stack.len();
        let (parent_path, index) = match self.stack.last_mut() {
            None => {
                if self.root.is_some() {
                    return Err(ErrorKind::UnexpectedNode {
                        node: "second root element".to_owned(),
                        position,
                    }
                    .into());
                }
                (None, 0)
            }
            Some(parent) => {
                if depth >= MAX_DEPTH {
                    return Err(ErrorKind::MaxDepthExceeded {
                        path: parent.path.to_string(),
                        position,
                        max_depth: MAX_DEPTH,
                    }
                    .into());
                }
                if parent.text_significant {
                    return Err(parent.mixed_content());
                }
                // Whitespace before the first child is not part of the text.
                parent.text.clear();
                // A previous sibling with the same local name has already ended,
                // so it is in `children`.
                let index = match parent.children.get(local_name) {
                    None => 0,
                    Some((_, siblings)) => {
                        let first_prefix = siblings.first().map_or("", |e| e.path.prefix.as_str());
                        if first_prefix != prefix {
                            return Err(ErrorKind::ChildPrefixMismatch {
                                path: parent.path.to_string(),
                                position,
                                name: local_name.to_owned(),
                                first_prefix: first_prefix.to_owned(),
                                second_prefix: prefix.to_owned(),
                            }
                            .into());
                        }
                        siblings.len()
                    }
                };
                (Some(parent.path.clone()), index)
            }
        };

        let path = Arc::new(ElemPath {
            parent: parent_path,
            name: local_name.to_owned(),
            prefix: prefix.to_owned(),
            index,
        });

        let mut attrs = BTreeMap::new();
        let mut attributes = start.attributes();
        // Duplicates are detected below by local name, which also covers equal qualified names.
        attributes.with_checks(false);
        for attr in attributes {
            // Positions in `AttrError` are relative to the byte after `<`.
            let attr = attr.map_err(|e| self.attr_error(e, offset + 1))?;
            let (attr_local, attr_prefix) = attr.key.decompose();
            let attr_local: &str = attr_local.as_ref();
            let is_namespace_decl = match &attr_prefix {
                None => attr_local == "xmlns",
                Some(p) => AsRef::<str>::as_ref(p) == "xmlns",
            };
            if is_namespace_decl {
                continue;
            }
            let value = match attr.normalized_value(XmlVersion::Implicit1_0) {
                Ok(value) => value,
                Err(e) => return Err(self.attr_value_error(e, &attr.value, &path, position)),
            };
            if attrs
                .insert(
                    attr_local.to_owned(),
                    (Cell::new(false), value.into_owned()),
                )
                .is_some()
            {
                return Err(ErrorKind::DuplicateAttr {
                    path: path.to_string(),
                    position,
                    name: attr_local.to_owned(),
                }
                .into());
            }
        }

        Ok(Builder {
            path,
            position,
            attrs,
            children: BTreeMap::new(),
            text: String::new(),
            text_significant: false,
        })
    }

    fn end_element(&mut self, builder: Builder) {
        let elem = builder.finish();
        match self.stack.last_mut() {
            None => self.root = Some(elem),
            Some(parent) => match parent.children.get_mut(elem.name()) {
                Some((_, siblings)) => siblings.push(elem),
                None => {
                    let name = elem.name().to_owned();
                    parent.children.insert(name, (Cell::new(false), vec![elem]));
                }
            },
        }
    }
}
