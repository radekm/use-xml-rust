//! Parsing of an XML document into a tree of [`Elem`].

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use quick_xml::XmlVersion;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::elem::{Elem, ElemPath};
use crate::error::{Error, Position};

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
    fn new(input: &'a str) -> Self {
        Self {
            input,
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
    attrs: BTreeMap<String, String>,
    /// A child is added when it ends. The builder receives events only while it is
    /// the innermost open element, i.e. when all its children have already ended,
    /// so `children` is non-empty iff the element has children.
    children: BTreeMap<String, Vec<Elem>>,
    text: String,
    text_significant: bool,
}

impl Builder {
    fn finish(self) -> Elem {
        let text = if self.children.is_empty() {
            self.text
        } else {
            String::new()
        };
        Elem {
            path: self.path,
            position: self.position,
            attrs: self.attrs,
            attrs_used: RefCell::new(BTreeSet::new()),
            children: self.children,
            children_used: RefCell::new(BTreeSet::new()),
            text,
            text_significant: self.text_significant,
            text_used: Cell::new(false),
            all_used: Cell::new(false),
        }
    }

    fn mixed_content(&self) -> Error {
        Error::MixedContent {
            path: self.path.to_string(),
            position: self.position,
        }
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
/// a UTF-16 byte order mark, an encoding other than UTF-8 in the XML declaration,
/// or invalid UTF-8 result in [`Error::Encoding`]. Otherwise it behaves like [`parse_str`].
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
    let start = Position { line: 1, column: 1 };
    if input.starts_with(&[0xFF, 0xFE]) || input.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::Encoding {
            message: "UTF-16 is not supported".to_owned(),
            position: start,
        });
    }
    let input = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input);
    let input = std::str::from_utf8(input).map_err(|e| {
        // The prefix up to `valid_up_to` is valid UTF-8.
        let valid = std::str::from_utf8(&input[..e.valid_up_to()]).unwrap_or_default();
        Error::Encoding {
            message: "invalid UTF-8".to_owned(),
            position: PositionTracker::new(valid).position_at(valid.len() as u64),
        }
    })?;
    parse(input, true)
}

fn parse(input: &str, check_encoding: bool) -> Result<Elem, Error> {
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
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
        Error::Xml {
            message,
            position: self.tracker.position_at(offset),
        }
    }

    fn unexpected(&mut self, node: &str, offset: u64) -> Error {
        Error::UnexpectedNode {
            node: node.to_owned(),
            position: self.tracker.position_at(offset),
        }
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
                    let text = text.xml10_content();
                    self.add_text(&text, false, offset, "text")?;
                }
                Event::CData(cdata) => {
                    let text = cdata.xml10_content();
                    self.add_text(&text, true, offset, "CDATA section")?;
                }
                Event::GeneralRef(reference) => {
                    let resolved = match reference.resolve_char_ref() {
                        Ok(Some(c)) => c.to_string(),
                        Ok(None) => match resolve_predefined_entity(&reference) {
                            Some(s) => s.to_owned(),
                            None => {
                                let name = reference.to_string();
                                return Err(match self.stack.last() {
                                    Some(builder) => Error::UnknownEntity {
                                        name,
                                        path: builder.path.to_string(),
                                        position: self.tracker.position_at(offset),
                                    },
                                    None => self.unexpected("entity reference", offset),
                                });
                            }
                        },
                        Err(e) => return Err(self.xml_error(e.to_string(), offset)),
                    };
                    self.add_text(&resolved, false, offset, "entity reference")?;
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
                                return Err(Error::Encoding {
                                    message: format!("encoding `{encoding}` is not supported"),
                                    position: self.tracker.position_at(offset),
                                });
                            }
                            Some(Err(e)) => return Err(self.xml_error(e.to_string(), offset)),
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
                    return Err(Error::UnexpectedNode {
                        node: "second root element".to_owned(),
                        position,
                    });
                }
                (None, 0)
            }
            Some(parent) => {
                if depth >= MAX_DEPTH {
                    return Err(Error::MaxDepthExceeded {
                        path: parent.path.to_string(),
                        position,
                        max_depth: MAX_DEPTH,
                    });
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
                    Some(siblings) => {
                        let first_prefix = siblings.first().map_or("", |e| e.path.prefix.as_str());
                        if first_prefix != prefix {
                            return Err(Error::ChildPrefixMismatch {
                                path: parent.path.to_string(),
                                position,
                                name: local_name.to_owned(),
                                first_prefix: first_prefix.to_owned(),
                                second_prefix: prefix.to_owned(),
                            });
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
        for attr in start.attributes() {
            let attr = attr.map_err(|e| Error::Xml {
                message: e.to_string(),
                position,
            })?;
            let (attr_local, attr_prefix) = attr.key.decompose();
            let attr_local: &str = attr_local.as_ref();
            let is_namespace_decl = match &attr_prefix {
                None => attr_local == "xmlns",
                Some(p) => AsRef::<str>::as_ref(p) == "xmlns",
            };
            if is_namespace_decl {
                continue;
            }
            let value = attr
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|e| Error::Xml {
                    message: e.to_string(),
                    position,
                })?;
            if attrs
                .insert(attr_local.to_owned(), value.into_owned())
                .is_some()
            {
                return Err(Error::DuplicateAttr {
                    path: path.to_string(),
                    position,
                    name: attr_local.to_owned(),
                });
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
                Some(siblings) => siblings.push(elem),
                None => {
                    parent.children.insert(elem.name().to_owned(), vec![elem]);
                }
            },
        }
    }
}
