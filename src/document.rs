//! The parsed document: an arena of element nodes and the interned names.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;

use crate::elem::Elem;
use crate::error::truncate_value;
use crate::error::{Error, ErrorKind, Limit, Position};
use crate::names::{
    MAX_NAMESPACE_LEN, NameTable, NamespaceHandle, QualifiedNameHandle, XMLNS_NAMESPACE,
};
use crate::parser;

/// Maximum nesting depth of elements. The root element has depth 1.
pub const MAX_DEPTH: usize = 256;

/// An element in the arena of a [`Document`].
pub(crate) struct Node {
    pub(crate) name: QualifiedNameHandle,
    /// Handle of the name as written in the document (with the prefix), used in paths.
    pub(crate) raw_name: u32,
    /// Index of the parent node; `None` for the root.
    pub(crate) parent: Option<u32>,
    /// 0-based index among the siblings with the same qualified name; 0 for the root.
    pub(crate) index: u32,
    /// Index of the last descendant (nodes are in preorder); the node itself if it has none.
    pub(crate) subtree_end: u32,
    pub(crate) position: Position,
    pub(crate) text_significant: bool,
    /// `ignore_all` was called on the element or on one of its ancestors.
    pub(crate) all_used: Cell<bool>,
    /// `None` after the text was used.
    pub(crate) text: Cell<Option<Box<str>>>,
    /// The value is `None` after the attribute was used or if a missing attribute was requested.
    pub(crate) attrs: RefCell<BTreeMap<QualifiedNameHandle, Option<Box<str>>>>,
    /// Children with each name in document order. An empty `Vec` means that the name was used
    /// (unused names always have at least one child; requested missing names are inserted empty).
    pub(crate) children: RefCell<BTreeMap<QualifiedNameHandle, Vec<u32>>>,
}

/// Formats the path of the node `index`, e.g. `/root/items[0]/item[2]`.
/// Uses no recursion.
pub(crate) fn format_path<'a>(
    nodes: &[Node],
    raw_name: impl Fn(u32) -> &'a str,
    index: u32,
) -> String {
    let mut segments = Vec::new();
    let mut current = nodes.get(index as usize);
    while let Some(node) = current {
        segments.push(node);
        current = node.parent.and_then(|p| nodes.get(p as usize));
    }
    let mut path = String::new();
    for node in segments.iter().rev() {
        path.push('/');
        path.push_str(raw_name(node.raw_name));
        if node.parent.is_some() {
            path.push('[');
            path.push_str(&node.index.to_string());
            path.push(']');
        }
    }
    path
}

/// A parsed XML document.
///
/// It owns all elements. The elements are accessed through [`Elem`] values
/// which borrow the document, starting with [`Document::root`].
///
/// `Document` is not `Sync` because usage of attributes, texts, and children
/// is tracked with interior mutability.
///
/// ```
/// use use_xml::parse;
///
/// let xml = r#"
/// <x xmlns:edi='http://ecommerce.example.org/schema'>
///   <lineItem edi:taxClass="exempt">Baby food</lineItem>
/// </x>
/// "#;
/// let doc = use_xml::parse_str(xml)?;
/// let default_ns = doc.namespace("")?;
/// let edi_ns = doc.namespace("http://ecommerce.example.org/schema")?;
/// let item = doc.root().child(default_ns, "lineItem")?;
/// assert_eq!(item.attr(edi_ns, "taxClass", parse::string)?, "exempt");
/// assert_eq!(item.text(parse::string)?, "Baby food");
/// doc.check_all_used()?;
/// # Ok::<(), use_xml::Error>(())
/// ```
pub struct Document {
    /// Elements in document order (preorder). The root is the first one.
    pub(crate) nodes: Vec<Node>,
    pub(crate) names: NameTable,
}

impl Document {
    /// Returns the root element.
    #[must_use]
    pub fn root(&self) -> Elem<'_> {
        // A document always has a root element, the parser fails otherwise.
        Elem::new(self, 0)
    }

    /// Returns the handle of the namespace `uri`. The empty string means no namespace.
    ///
    /// A namespace which does not occur in the document is interned too,
    /// so the handle can be used to request (missing) attributes and children.
    /// The namespace name is checked like a namespace name in the document,
    /// but it is not checked to be a URI.
    ///
    /// ```
    /// let doc = use_xml::parse_str("<a xmlns='urn:a'/>")?;
    /// assert_eq!(doc.root().namespace(), doc.namespace("urn:a")?);
    /// assert!(doc.namespace("urn:\u{0}").is_err());
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::LimitExceeded`] with [`Limit::Namespaces`] if the document would have
    ///   more than [`MAX_NAMESPACES`](crate::MAX_NAMESPACES) namespaces,
    ///   or with [`Limit::NamespaceLen`] if `uri` is longer than [`MAX_NAMESPACE_LEN`] bytes.
    /// - [`ErrorKind::InvalidNamespace`] if `uri` contains a character which is not allowed
    ///   in XML or if it is `http://www.w3.org/2000/xmlns/`, which must not be declared.
    pub fn namespace(&self, uri: &str) -> Result<NamespaceHandle, Error> {
        if uri.len() > MAX_NAMESPACE_LEN {
            return Err(ErrorKind::LimitExceeded {
                limit: Limit::NamespaceLen,
                position: None,
            }
            .into());
        }
        let invalid = |message: String| -> Error {
            ErrorKind::InvalidNamespace {
                namespace: truncate_value(uri).into_owned(),
                message,
            }
            .into()
        };
        if let Some(c) = uri.chars().find(|&c| !parser::is_xml_char(c)) {
            return Err(invalid(format!(
                "forbidden character U+{:04X}",
                u32::from(c)
            )));
        }
        if uri == XMLNS_NAMESPACE {
            return Err(invalid(format!(
                "namespace `{XMLNS_NAMESPACE}` must not be declared"
            )));
        }
        self.names.namespace(uri).map_err(|limit| {
            ErrorKind::LimitExceeded {
                limit,
                position: None,
            }
            .into()
        })
    }

    /// Checks that everything in the document was used. See [`Elem::check_all_used`].
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Unused`] for the first unused item.
    pub fn check_all_used(&self) -> Result<(), Error> {
        self.root().check_all_used()
    }

    pub(crate) fn node(&self, index: u32) -> &Node {
        // `Elem` values are created only with indices of existing nodes
        // and nodes are never removed.
        &self.nodes[index as usize]
    }

    pub(crate) fn path(&self, index: u32) -> String {
        format_path(&self.nodes, |h| self.names.raw_name(h), index)
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("elements", &self.nodes.len())
            .field("root", &self.root())
            .finish()
    }
}

/// Parses an XML document.
///
/// The document must be well-formed and namespace-well-formed XML 1.0
/// and follow the rules of this library (no `DOCTYPE`, no mixed content, limits).
/// A leading byte order mark is skipped. The encoding in the XML declaration is checked
/// only syntactically because the input is already decoded.
///
/// ```
/// let doc = use_xml::parse_str("<animals><dog>A</dog><dog>D</dog></animals>")?;
/// let ns = doc.namespace("")?;
/// assert_eq!(doc.root().local_name(), "animals");
/// assert_eq!(doc.root().children(ns, "dog")?.len(), 2);
/// # Ok::<(), use_xml::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if the document is not well-formed or violates a rule of this library.
pub fn parse_str(input: &str) -> Result<Document, Error> {
    parser::parse(input, false)
}

/// Parses an XML document encoded in UTF-8.
///
/// A leading UTF-8 byte order mark is skipped. Other encodings are not supported:
/// UTF-16 or UTF-32 (detected by a byte order mark or by the encoded `<` at the start),
/// an encoding other than UTF-8 in the XML declaration,
/// or invalid UTF-8 result in [`ErrorKind::Encoding`]. Otherwise it behaves like [`parse_str`].
///
/// ```
/// let doc = use_xml::parse_bytes(b"<?xml version='1.0' encoding='UTF-8'?><a x='1'/>")?;
/// assert_eq!(doc.root().attr(use_xml::NamespaceHandle::NO_NAMESPACE, "x", use_xml::parse::u8)?, 1);
/// # Ok::<(), use_xml::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error if the input is not UTF-8, the document is not well-formed,
/// or it violates a rule of this library.
pub fn parse_bytes(input: &[u8]) -> Result<Document, Error> {
    if let Some(encoding) = detect_unsupported_encoding(input) {
        return Err(ErrorKind::Encoding {
            message: format!("{encoding} is not supported"),
            position: Position { line: 1, column: 1 },
        }
        .into());
    }
    let input = std::str::from_utf8(input).map_err(|e| -> Error {
        // The prefix up to `valid_up_to` is valid UTF-8.
        let valid = std::str::from_utf8(&input[..e.valid_up_to()]).unwrap_or_default();
        let valid = valid.strip_prefix('\u{FEFF}').unwrap_or(valid);
        ErrorKind::Encoding {
            message: "invalid UTF-8".to_owned(),
            position: parser::PositionTracker::new(valid).position_at(valid.len()),
        }
        .into()
    })?;
    parser::parse(input, true)
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
