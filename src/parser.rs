//! A checking parser of XML 1.0 with namespaces.
//!
//! The parser accepts only documents which are well-formed and namespace-well-formed
//! and which follow the rules of this library (see the design documentation).
//! It uses no recursion: open elements are kept on an explicit stack.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use crate::document::{Document, MAX_DEPTH, Node, format_path, intern_limit};
use crate::error::{Error, ErrorKind, Limit, Position};
use crate::names::{
    Interner, MAX_ATTRS, MAX_CHILD_NAMES, MAX_NAME_LEN, MAX_NAMESPACE_LEN, MAX_PREFIX_LEN,
    NamespaceHandle, XML_NAMESPACE, XMLNS_NAMESPACE,
};

/// Message for a reference without the closing `;`, in text as well as in attribute values.
const UNCLOSED_REFERENCE: &str = "entity or character reference not closed: `;` not found";

/// Converts byte offsets into lines and columns.
///
/// Offsets are usually requested in increasing order, so the tracker remembers
/// the last position and continues from it.
pub(crate) struct PositionTracker<'a> {
    input: &'a str,
    offset: usize,
    position: Position,
}

impl<'a> PositionTracker<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input,
            offset: 0,
            position: Position { line: 1, column: 1 },
        }
    }

    pub(crate) fn position_at(&mut self, offset: usize) -> Position {
        let mut offset = offset.min(self.input.len());
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

/// The `Char` production of XML 1.0.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// `NameStartChar` of XML 1.0 without `:` (namespaces do not allow colons in `NCName`).
fn is_name_start_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

/// `NameChar` of XML 1.0 without `:`.
fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Whether `s` matches the `NCName` production of Namespaces in XML 1.0.
pub(crate) fn is_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(is_name_start_char) && chars.all(is_name_char)
}

fn is_whitespace_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_whitespace(s: &str) -> bool {
    s.bytes().all(is_whitespace_byte)
}

/// Appends `src` to `dst` and normalizes line endings (`\r\n` and `\r` become `\n`).
fn push_normalized(dst: &mut String, src: &str) {
    if !src.contains('\r') {
        dst.push_str(src);
        return;
    }
    let mut rest = src;
    while let Some(i) = rest.find('\r') {
        dst.push_str(&rest[..i]);
        dst.push('\n');
        rest = &rest[i + 1..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    dst.push_str(rest);
}

/// A qualified name as written in the document.
#[derive(Clone, Copy)]
struct RawName<'a> {
    raw: &'a str,
    prefix: &'a str,
    local: &'a str,
}

struct RawAttr<'a> {
    name: RawName<'a>,
    value: Cow<'a, str>,
    offset: usize,
}

/// An element whose end tag was not reached yet.
struct Open<'a> {
    node: u32,
    raw_name: &'a str,
    text: String,
    /// Offset of the first significant text (its first non-whitespace character,
    /// `<![CDATA[`, or `&` of a reference), `None` if the text is not significant.
    significant_at: Option<usize>,
    has_children: bool,
    /// Length of `Parser::undo` before the declarations of this element.
    undo_len: usize,
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tracker: PositionTracker<'a>,
    interner: Interner,
    nodes: Vec<Node>,
    stack: Vec<Open<'a>>,
    /// In-scope namespace bindings. The prefix `""` is the default namespace.
    bindings: BTreeMap<&'a str, NamespaceHandle>,
    /// Bindings replaced by declarations of open elements (`None` if the prefix was not bound).
    /// They are restored at the end of the declaring element.
    undo: Vec<(&'a str, Option<NamespaceHandle>)>,
    /// Reused for the attributes of each start tag.
    raw_attrs: Vec<RawAttr<'a>>,
    /// Whether the encoding in the XML declaration must be UTF-8.
    check_encoding: bool,
}

/// Parses a whole document. `check_encoding` is true for `parse_bytes`.
pub(crate) fn parse(input: &str, check_encoding: bool) -> Result<Document, Error> {
    // A single leading byte order mark is skipped. Positions are counted after it.
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let mut parser = Parser {
        input,
        bytes: input.as_bytes(),
        pos: 0,
        tracker: PositionTracker::new(input),
        interner: Interner::new(),
        nodes: Vec::new(),
        stack: Vec::new(),
        bindings: BTreeMap::from([("xml", NamespaceHandle::XML)]),
        undo: Vec::new(),
        raw_attrs: Vec::new(),
        check_encoding,
    };
    parser.check_chars()?;
    parser.xml_declaration()?;
    parser.misc(false)?;
    parser.root_element()?;
    parser.misc(true)?;
    Ok(Document {
        nodes: parser.nodes,
        names: parser.interner.finish(),
    })
}

impl<'a> Parser<'a> {
    // ----- Helpers -----

    fn position(&mut self, offset: usize) -> Position {
        self.tracker.position_at(offset)
    }

    fn xml_error(&mut self, offset: usize, message: impl Into<String>) -> Error {
        ErrorKind::Xml {
            message: message.into(),
            position: self.position(offset),
        }
        .into()
    }

    fn namespace_error(&mut self, offset: usize, message: impl Into<String>) -> Error {
        ErrorKind::Namespace {
            message: message.into(),
            position: self.position(offset),
        }
        .into()
    }

    fn limit_error(&mut self, offset: usize, limit: Limit) -> Error {
        ErrorKind::LimitExceeded {
            limit,
            position: Some(self.position(offset)),
        }
        .into()
    }

    fn path(&self, node: u32) -> String {
        format_path(&self.nodes, |h| self.interner.get_raw_name(h), node)
    }

    fn starts_with(&self, s: &str) -> bool {
        self.bytes[self.pos..].starts_with(s.as_bytes())
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Skips whitespace and returns whether there was any.
    fn skip_whitespace(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(is_whitespace_byte) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn expect(&mut self, s: &str, message: &str) -> Result<(), Error> {
        if self.starts_with(s) {
            self.pos += s.len();
            Ok(())
        } else {
            Err(self.xml_error(self.pos, message))
        }
    }

    /// Finds `s` at or after the current position and returns its offset.
    fn find(&self, s: &str) -> Option<usize> {
        self.input[self.pos..].find(s).map(|i| self.pos + i)
    }

    /// Checks that the input contains only characters allowed by XML 1.0.
    fn check_chars(&mut self) -> Result<(), Error> {
        let bytes = self.bytes;
        for (i, &b) in bytes.iter().enumerate() {
            let forbidden = (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r'))
                // U+FFFE and U+FFFF are encoded as EF BF BE and EF BF BF.
                || (b == 0xEF
                    && bytes.get(i + 1) == Some(&0xBF)
                    && matches!(bytes.get(i + 2), Some(0xBE | 0xBF)));
            if forbidden {
                let c = self.input[i..].chars().next().unwrap_or_default();
                return Err(
                    self.xml_error(i, format!("forbidden character U+{:04X}", u32::from(c)))
                );
            }
        }
        Ok(())
    }

    /// Returns the end of the `NCName` starting at `start` (`start` if there is none).
    fn ncname_end(&self, start: usize) -> usize {
        let mut chars = self.input[start..].char_indices();
        match chars.next() {
            Some((_, c)) if is_name_start_char(c) => {}
            _ => return start,
        }
        for (i, c) in chars {
            if !is_name_char(c) {
                return start + i;
            }
        }
        self.input.len()
    }

    /// Reads an `NCName` (a name without colons), e.g. a processing instruction target.
    fn ncname(&mut self, what: &str) -> Result<&'a str, Error> {
        let start = self.pos;
        let end = self.ncname_end(start);
        if end == start {
            return Err(self.xml_error(start, format!("expected {what}")));
        }
        if self.bytes.get(end) == Some(&b':') {
            return Err(self.namespace_error(start, format!("colon in {what}")));
        }
        if end - start > MAX_NAME_LEN {
            return Err(self.limit_error(start, Limit::NameLen));
        }
        self.pos = end;
        Ok(&self.input[start..end])
    }

    /// Reads a qualified name `prefix:local` or `local`.
    ///
    /// For an attribute `xmlns:prefix` (a namespace declaration) the local part is the declared
    /// prefix, so it is limited by `MAX_PREFIX_LEN` instead of `MAX_NAME_LEN`.
    fn qname(&mut self, what: &str, attribute: bool) -> Result<RawName<'a>, Error> {
        let start = self.pos;
        let first_end = self.ncname_end(start);
        if first_end == start {
            return Err(if self.peek() == Some(b':') {
                self.namespace_error(start, format!("{what} starts with a colon"))
            } else {
                self.xml_error(start, format!("expected {what}"))
            });
        }
        let (prefix, local_start, end) = if self.bytes.get(first_end) == Some(&b':') {
            let local_start = first_end + 1;
            let end = self.ncname_end(local_start);
            if end == local_start {
                return Err(self
                    .namespace_error(start, format!("{what} has an empty or invalid local name")));
            }
            (&self.input[start..first_end], local_start, end)
        } else {
            ("", start, first_end)
        };
        if self.bytes.get(end) == Some(&b':') {
            return Err(self.namespace_error(start, format!("{what} contains multiple colons")));
        }
        if prefix.len() > MAX_PREFIX_LEN {
            return Err(self.limit_error(start, Limit::PrefixLen));
        }
        let (max_local_len, local_limit) = if attribute && prefix == "xmlns" {
            (MAX_PREFIX_LEN, Limit::PrefixLen)
        } else {
            (MAX_NAME_LEN, Limit::NameLen)
        };
        if end - local_start > max_local_len {
            return Err(self.limit_error(start, local_limit));
        }
        self.pos = end;
        Ok(RawName {
            raw: &self.input[start..end],
            prefix,
            local: &self.input[local_start..end],
        })
    }

    /// Reads a quoted value of a pseudo-attribute of the XML declaration.
    fn declaration_value(&mut self, name: &str) -> Result<(usize, &'a str), Error> {
        if !self.starts_with(name) {
            return Err(self.xml_error(self.pos, format!("expected `{name}` in XML declaration")));
        }
        self.pos += name.len();
        self.skip_whitespace();
        self.expect("=", "expected `=` in XML declaration")?;
        self.skip_whitespace();
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.xml_error(self.pos, "expected quoted value in XML declaration")),
        };
        let start = self.pos + 1;
        let Some(len) = self.bytes[start..].iter().position(|&b| b == quote) else {
            return Err(self.xml_error(self.pos, "value in XML declaration not closed"));
        };
        self.pos = start + len + 1;
        Ok((start, &self.input[start..start + len]))
    }

    // ----- Document structure -----

    /// Parses the XML declaration if the document starts with it.
    fn xml_declaration(&mut self) -> Result<(), Error> {
        if !(self.starts_with("<?xml")
            && self
                .bytes
                .get(5)
                .is_some_and(|&b| is_whitespace_byte(b) || b == b'?'))
        {
            return Ok(());
        }
        self.pos += 5;
        self.skip_whitespace();
        let (start, version) = self.declaration_value("version")?;
        let valid_version = version
            .strip_prefix("1.")
            .is_some_and(|minor| !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit()));
        if !valid_version {
            return Err(self.xml_error(start, format!("unsupported XML version `{version}`")));
        }
        let mut whitespace = self.skip_whitespace();
        if whitespace && self.starts_with("encoding") {
            let (start, encoding) = self.declaration_value("encoding")?;
            let mut bytes = encoding.bytes();
            let valid_name = bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
                && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
            if !valid_name {
                return Err(self.xml_error(start, format!("invalid encoding name `{encoding}`")));
            }
            if self.check_encoding && !encoding.eq_ignore_ascii_case("UTF-8") {
                return Err(ErrorKind::Encoding {
                    message: format!("encoding `{encoding}` is not supported"),
                    position: self.position(start),
                }
                .into());
            }
            whitespace = self.skip_whitespace();
        }
        if whitespace && self.starts_with("standalone") {
            let (start, standalone) = self.declaration_value("standalone")?;
            if standalone != "yes" && standalone != "no" {
                return Err(self.xml_error(start, "`standalone` must be `yes` or `no`"));
            }
            self.skip_whitespace();
        }
        self.expect("?>", "expected `?>` at the end of XML declaration")
    }

    /// Parses comments, processing instructions, and whitespace before or after the root element.
    /// Before the root it stops at the start tag of the root.
    fn misc(&mut self, after_root: bool) -> Result<(), Error> {
        loop {
            self.skip_whitespace();
            if self.at_end() {
                if after_root {
                    return Ok(());
                }
                return Err(self.xml_error(self.pos, "missing root element"));
            }
            let node = if self.starts_with("<!--") {
                self.comment()?;
                continue;
            } else if self.starts_with("<?") {
                self.processing_instruction()?;
                continue;
            } else if self.starts_with("<!DOCTYPE") {
                "DOCTYPE"
            } else if self.starts_with("<![CDATA[") {
                "CDATA section outside of the root element"
            } else if self.starts_with("</") {
                return Err(self.xml_error(self.pos, "end tag without start tag"));
            } else if self.starts_with("<!") {
                return Err(self.xml_error(self.pos, "invalid markup"));
            } else if self.starts_with("<") {
                if !after_root {
                    return Ok(());
                }
                "second root element"
            } else if self.starts_with("&") {
                "entity reference outside of the root element"
            } else {
                "text outside of the root element"
            };
            return Err(ErrorKind::UnexpectedNode {
                node: node.to_owned(),
                position: self.position(self.pos),
            }
            .into());
        }
    }

    fn comment(&mut self) -> Result<(), Error> {
        let start = self.pos;
        self.pos += 4;
        let Some(i) = self.find("--") else {
            return Err(self.xml_error(start, "comment not closed"));
        };
        if self.bytes.get(i + 2) != Some(&b'>') {
            return Err(self.xml_error(i, "`--` in comment"));
        }
        self.pos = i + 3;
        Ok(())
    }

    fn processing_instruction(&mut self) -> Result<(), Error> {
        let start = self.pos;
        self.pos += 2;
        let target = self.ncname("processing instruction target")?;
        if target == "xml" {
            return Err(ErrorKind::UnexpectedNode {
                node: "XML declaration".to_owned(),
                position: self.position(start),
            }
            .into());
        }
        if target.eq_ignore_ascii_case("xml") {
            return Err(self.xml_error(
                start,
                format!("reserved processing instruction target `{target}`"),
            ));
        }
        if !self.starts_with("?>") && !self.skip_whitespace() && !self.at_end() {
            return Err(self.xml_error(
                self.pos,
                "expected whitespace after processing instruction target",
            ));
        }
        let Some(i) = self.find("?>") else {
            return Err(self.xml_error(start, "processing instruction not closed"));
        };
        self.pos = i + 2;
        Ok(())
    }

    // ----- Elements -----

    fn root_element(&mut self) -> Result<(), Error> {
        self.start_tag()?;
        while let Some(top) = self.stack.last() {
            let Some(b) = self.peek() else {
                let message = format!("element `{}` not closed", top.raw_name);
                return Err(self.xml_error(self.pos, message));
            };
            match b {
                b'<' => {
                    if self.starts_with("</") {
                        self.end_tag()?;
                    } else if self.starts_with("<!--") {
                        self.comment()?;
                    } else if self.starts_with("<?") {
                        self.processing_instruction()?;
                    } else if self.starts_with("<![CDATA[") {
                        self.cdata()?;
                    } else if self.starts_with("<!") {
                        return Err(self.xml_error(self.pos, "invalid markup in element content"));
                    } else {
                        self.start_tag()?;
                    }
                }
                b'&' => {
                    let amp = self.pos;
                    let mut text = String::new();
                    self.reference(&mut text)?;
                    self.add_text(&text, false, amp)?;
                }
                _ => self.char_data()?,
            }
        }
        Ok(())
    }

    fn char_data(&mut self) -> Result<(), Error> {
        let start = self.pos;
        let len = self.bytes[start..]
            .iter()
            .position(|&b| b == b'<' || b == b'&')
            .unwrap_or(self.bytes.len() - start);
        let raw = &self.input[start..start + len];
        if let Some(i) = raw.find("]]>") {
            return Err(self.xml_error(start + i, "`]]>` in text"));
        }
        self.pos = start + len;
        // The offset of the first non-whitespace character (used only if there is one).
        let offset = start
            + raw
                .bytes()
                .position(|b| !is_whitespace_byte(b))
                .unwrap_or(0);
        if raw.contains('\r') {
            let mut text = String::with_capacity(raw.len());
            push_normalized(&mut text, raw);
            self.add_text(&text, false, offset)
        } else {
            self.add_text(raw, false, offset)
        }
    }

    fn cdata(&mut self) -> Result<(), Error> {
        let start = self.pos;
        self.pos += "<![CDATA[".len();
        let Some(end) = self.find("]]>") else {
            return Err(self.xml_error(start, "CDATA section not closed"));
        };
        let raw = &self.input[self.pos..end];
        self.pos = end + 3;
        let mut text = String::new();
        push_normalized(&mut text, raw);
        self.add_text(&text, true, start)
    }

    /// Adds text to the innermost open element. CDATA is always significant,
    /// other text if it contains a non-whitespace character.
    /// `offset` is where significant text starts (for errors).
    fn add_text(&mut self, text: &str, cdata: bool, offset: usize) -> Result<(), Error> {
        let significant = cdata || !is_whitespace(text);
        let Some(top) = self.stack.last_mut() else {
            return Ok(());
        };
        if significant {
            if top.has_children {
                let node = top.node;
                return Err(self.mixed_content(node, offset));
            }
            top.significant_at.get_or_insert(offset);
            top.text.push_str(text);
        } else if !top.has_children {
            // Whitespace is kept only if the element has no children.
            top.text.push_str(text);
        }
        Ok(())
    }

    /// Mixed content in `node`; `offset` is the significant text.
    fn mixed_content(&mut self, node: u32, offset: usize) -> Error {
        ErrorKind::MixedContent {
            path: self.path(node),
            position: self.position(offset),
        }
        .into()
    }

    /// Parses a reference at `&` and appends the referenced character(s) to `out`.
    fn reference(&mut self, out: &mut String) -> Result<(), Error> {
        let amp = self.pos;
        self.pos += 1;
        if self.peek() == Some(b'#') {
            self.pos += 1;
            let hex = self.peek() == Some(b'x');
            if hex {
                self.pos += 1;
            }
            let digits_start = self.pos;
            while self.peek().is_some_and(|b| {
                if hex {
                    b.is_ascii_hexdigit()
                } else {
                    b.is_ascii_digit()
                }
            }) {
                self.pos += 1;
            }
            let digits = &self.input[digits_start..self.pos];
            if digits.is_empty() {
                return Err(self.xml_error(amp, "invalid character reference"));
            }
            if self.peek() != Some(b';') {
                return Err(self.xml_error(amp, UNCLOSED_REFERENCE));
            }
            self.pos += 1;
            let c = u32::from_str_radix(digits, if hex { 16 } else { 10 })
                .ok()
                .and_then(char::from_u32)
                .filter(|&c| is_xml_char(c));
            match c {
                Some(c) => out.push(c),
                None => {
                    return Err(self.xml_error(amp, "character reference to a forbidden character"));
                }
            }
            return Ok(());
        }
        let name_start = self.pos;
        let name_end = self.ncname_end(name_start);
        if name_end == name_start {
            return Err(self.xml_error(amp, "invalid reference"));
        }
        if self.bytes.get(name_end) == Some(&b':') {
            return Err(self.namespace_error(amp, "colon in entity name"));
        }
        if name_end - name_start > MAX_NAME_LEN {
            return Err(self.limit_error(amp, Limit::NameLen));
        }
        if self.bytes.get(name_end) != Some(&b';') {
            return Err(self.xml_error(amp, UNCLOSED_REFERENCE));
        }
        self.pos = name_end + 1;
        let name = &self.input[name_start..name_end];
        out.push(match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "apos" => '\'',
            "quot" => '"',
            _ => {
                return Err(ErrorKind::UnknownEntity {
                    name: name.to_owned(),
                    position: self.position(amp),
                }
                .into());
            }
        });
        Ok(())
    }

    /// Parses a quoted attribute value and normalizes it (whitespace characters become spaces).
    fn attr_value(&mut self) -> Result<Cow<'a, str>, Error> {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.xml_error(self.pos, "expected quoted attribute value")),
        };
        let open = self.pos;
        self.pos += 1;
        let start = self.pos;
        let special = |b: u8| b == quote || matches!(b, b'<' | b'&' | b'\t' | b'\n' | b'\r');
        // Fast path: a value without references and whitespace other than spaces.
        let plain_len = self.bytes[start..].iter().position(|&b| special(b));
        if let Some(len) = plain_len
            && self.bytes[start + len] == quote
        {
            self.pos = start + len + 1;
            return Ok(Cow::Borrowed(&self.input[start..start + len]));
        }
        let mut value = String::new();
        loop {
            let run_start = self.pos;
            let Some(len) = self.bytes[run_start..].iter().position(|&b| special(b)) else {
                return Err(self.xml_error(open, "attribute value not closed"));
            };
            value.push_str(&self.input[run_start..run_start + len]);
            self.pos = run_start + len;
            match self.bytes[self.pos] {
                b if b == quote => {
                    self.pos += 1;
                    return Ok(Cow::Owned(value));
                }
                b'<' => return Err(self.xml_error(self.pos, "`<` in attribute value")),
                b'&' => self.reference(&mut value)?,
                b'\r' => {
                    value.push(' ');
                    self.pos += 1;
                    if self.peek() == Some(b'\n') {
                        self.pos += 1;
                    }
                }
                _ => {
                    // `\t` or `\n`.
                    value.push(' ');
                    self.pos += 1;
                }
            }
        }
    }

    fn lookup_prefix(&self, prefix: &str) -> Option<NamespaceHandle> {
        self.bindings.get(prefix).copied()
    }

    /// Restores the bindings replaced by the declarations recorded after `undo_len`.
    fn restore_bindings(&mut self, undo_len: usize) {
        while self.undo.len() > undo_len {
            let Some((prefix, previous)) = self.undo.pop() else {
                break;
            };
            match previous {
                Some(ns) => self.bindings.insert(prefix, ns),
                None => self.bindings.remove(prefix),
            };
        }
    }

    /// Checks a namespace declaration and adds the binding.
    fn declare_namespace(
        &mut self,
        prefix: &'a str,
        uri: &str,
        offset: usize,
    ) -> Result<(), Error> {
        if prefix.len() > MAX_PREFIX_LEN {
            return Err(self.limit_error(offset, Limit::PrefixLen));
        }
        if uri.len() > MAX_NAMESPACE_LEN {
            return Err(self.limit_error(offset, Limit::NamespaceLen));
        }
        let error = match prefix {
            "xmlns" => Some("prefix `xmlns` must not be declared".to_owned()),
            "xml" if uri != XML_NAMESPACE => {
                Some(format!("prefix `xml` must be bound to `{XML_NAMESPACE}`"))
            }
            _ if prefix != "xml" && uri == XML_NAMESPACE => Some(format!(
                "namespace `{XML_NAMESPACE}` must be bound only to prefix `xml`"
            )),
            _ if uri == XMLNS_NAMESPACE => Some(format!(
                "namespace `{XMLNS_NAMESPACE}` must not be declared"
            )),
            _ if !prefix.is_empty() && uri.is_empty() => Some(format!(
                "prefix `{prefix}` must not be bound to the empty namespace"
            )),
            _ => None,
        };
        if let Some(message) = error {
            return Err(self.namespace_error(offset, message));
        }
        let ns = self
            .interner
            .namespace(uri)
            .map_err(|e| self.limit_error(offset, intern_limit(e)))?;
        let previous = self.bindings.insert(prefix, ns);
        self.undo.push((prefix, previous));
        Ok(())
    }

    /// Parses a start tag at `<`, creates its node, and opens it unless it is empty.
    fn start_tag(&mut self) -> Result<(), Error> {
        let tag_start = self.pos;
        if let Some(top) = self.stack.last() {
            let parent = top.node;
            if self.stack.len() >= MAX_DEPTH {
                return Err(ErrorKind::MaxDepthExceeded {
                    path: self.path(parent),
                    position: self.position(tag_start),
                    max_depth: MAX_DEPTH,
                }
                .into());
            }
            if let Some(offset) = top.significant_at {
                return Err(self.mixed_content(parent, offset));
            }
        }
        let position = self.position(tag_start);
        self.pos += 1;
        let name = self.qname("element name", false)?;

        // Attributes.
        let mut raw_attrs = std::mem::take(&mut self.raw_attrs);
        raw_attrs.clear();
        let empty = loop {
            let whitespace = self.skip_whitespace();
            if self.starts_with("/>") {
                self.pos += 2;
                break true;
            }
            if self.starts_with(">") {
                self.pos += 1;
                break false;
            }
            if self.at_end() {
                return Err(
                    self.xml_error(tag_start, format!("start tag `{}` not closed", name.raw))
                );
            }
            if !whitespace {
                return Err(self.xml_error(self.pos, "expected whitespace, `>`, or `/>`"));
            }
            let offset = self.pos;
            if raw_attrs.len() >= MAX_ATTRS {
                return Err(self.limit_error(offset, Limit::Attrs));
            }
            let attr_name = self.qname("attribute name", true)?;
            self.skip_whitespace();
            self.expect("=", "expected `=` after attribute name")?;
            self.skip_whitespace();
            let value = self.attr_value()?;
            raw_attrs.push(RawAttr {
                name: attr_name,
                value,
                offset,
            });
        };

        // Namespace declarations.
        let undo_len = self.undo.len();
        for attr in &raw_attrs {
            if attr.name.raw == "xmlns" {
                self.declare_namespace("", &attr.value, attr.offset)?;
            } else if attr.name.prefix == "xmlns" {
                self.declare_namespace(attr.name.local, &attr.value, attr.offset)?;
            }
        }

        // The element.
        if name.prefix == "xmlns" {
            return Err(self.namespace_error(
                tag_start + 1,
                "prefix `xmlns` must not be used for elements",
            ));
        }
        let ns = match self.lookup_prefix(name.prefix) {
            Some(ns) => ns,
            None if name.prefix.is_empty() => NamespaceHandle::NO_NAMESPACE,
            None => {
                let message = format!("undeclared prefix `{}`", name.prefix);
                return Err(self.namespace_error(tag_start + 1, message));
            }
        };
        let qname = self
            .interner
            .name(ns, name.local)
            .map_err(|e| self.limit_error(tag_start, intern_limit(e)))?;
        let raw_name = self
            .interner
            .raw_name(name.raw)
            .map_err(|e| self.limit_error(tag_start, intern_limit(e)))?;
        let Ok(node_index) = u32::try_from(self.nodes.len()) else {
            return Err(self.xml_error(tag_start, "too many elements"));
        };
        let (parent, index) = match self.stack.last_mut() {
            None => (None, 0),
            Some(top) => {
                top.has_children = true;
                top.text.clear();
                let parent = top.node;
                let children = self.nodes[parent as usize].children.get_mut();
                if !children.contains_key(&qname) && children.len() >= MAX_CHILD_NAMES {
                    return Err(self.limit_error(tag_start, Limit::ChildNames));
                }
                let (_, siblings) = children.entry(qname).or_insert_with(|| (false, Vec::new()));
                siblings.push(node_index);
                // The number of siblings is at most the number of nodes, which fits into `u32`.
                (
                    Some(parent),
                    u32::try_from(siblings.len()).unwrap_or(u32::MAX),
                )
            }
        };
        self.nodes.push(Node {
            name: qname,
            raw_name,
            parent,
            index,
            position,
            text_significant: false,
            all_used: Cell::new(false),
            text: Cell::new(Some(Box::default())),
            attrs: RefCell::new(BTreeMap::new()),
            children: RefCell::new(BTreeMap::new()),
        });

        // Attributes of the element.
        let mut declared = BTreeSet::new();
        let mut attrs = BTreeMap::new();
        for attr in raw_attrs.drain(..) {
            let duplicate = if attr.name.raw == "xmlns" || attr.name.prefix == "xmlns" {
                !declared.insert(attr.name.raw)
            } else {
                let ns = if attr.name.prefix.is_empty() {
                    NamespaceHandle::NO_NAMESPACE
                } else if let Some(ns) = self.lookup_prefix(attr.name.prefix) {
                    ns
                } else {
                    let message = format!("undeclared prefix `{}`", attr.name.prefix);
                    return Err(self.namespace_error(attr.offset, message));
                };
                let qname = self
                    .interner
                    .name(ns, attr.name.local)
                    .map_err(|e| self.limit_error(attr.offset, intern_limit(e)))?;
                attrs
                    .insert(qname, Some(attr.value.into_owned().into_boxed_str()))
                    .is_some()
            };
            if duplicate {
                return Err(ErrorKind::DuplicateAttr {
                    path: self.path(node_index),
                    position: self.position(attr.offset),
                    name: attr.name.raw.to_owned(),
                }
                .into());
            }
        }
        self.raw_attrs = raw_attrs;
        if let Some(node) = self.nodes.last_mut() {
            *node.attrs.get_mut() = attrs;
        }

        if empty {
            self.restore_bindings(undo_len);
        } else {
            self.stack.push(Open {
                node: node_index,
                raw_name: name.raw,
                text: String::new(),
                significant_at: None,
                has_children: false,
                undo_len,
            });
        }
        Ok(())
    }

    fn end_tag(&mut self) -> Result<(), Error> {
        let tag_start = self.pos;
        self.pos += 2;
        let name = self.qname("element name", false)?;
        self.skip_whitespace();
        self.expect(">", "expected `>` at the end of end tag")?;
        let Some(top) = self.stack.pop() else {
            return Err(self.xml_error(tag_start, "end tag without start tag"));
        };
        if top.raw_name != name.raw {
            return Err(self.xml_error(
                tag_start,
                format!(
                    "end tag `{}` does not match start tag `{}`",
                    name.raw, top.raw_name
                ),
            ));
        }
        self.restore_bindings(top.undo_len);
        let node = &mut self.nodes[top.node as usize];
        node.text_significant = top.significant_at.is_some();
        if !top.has_children {
            *node.text.get_mut() = Some(top.text.into_boxed_str());
        }
        Ok(())
    }
}
