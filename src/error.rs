//! The error type of the library.

use std::borrow::Cow;
use std::fmt;

use crate::names::QualifiedName;

/// Maximum number of characters of a value shown in messages of the parsers
/// from the [`parse`](crate::parse) module, in [`ErrorKind::InvalidName`]
/// and [`ErrorKind::InvalidNamespace`], and by the `Debug` implementation of [`Elem`](crate::Elem).
/// Longer values are cut and end with `…`.
pub const MAX_VALUE_CHARS: usize = 100;

/// Returns at most [`MAX_VALUE_CHARS`] characters of `value`, followed by `…` if it was cut.
pub(crate) fn truncate_value(value: &str) -> Cow<'_, str> {
    match value.char_indices().nth(MAX_VALUE_CHARS) {
        None => Cow::Borrowed(value),
        // `i` is the index of a character, so it is on a character boundary.
        Some((i, _)) => Cow::Owned(format!("{}…", &value[..i])),
    }
}

/// A position in the source document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: u32,
    /// Column number, starting at 1, counted in Unicode scalar values.
    pub column: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// A part of an element which can be used.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// An attribute with the given qualified name.
    Attr(QualifiedName),
    /// Children with the given qualified name.
    Child(QualifiedName),
    /// The text content of the element.
    Text,
}

impl Item {
    /// An attribute with the given namespace name and local name.
    pub fn attr(namespace: &str, local_name: &str) -> Self {
        Item::Attr(QualifiedName::new(namespace, local_name))
    }

    /// Children with the given namespace name and local name.
    pub fn child(namespace: &str, local_name: &str) -> Self {
        Item::Child(QualifiedName::new(namespace, local_name))
    }
}

impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Item::Attr(name) => write!(f, "attribute `{name}`"),
            Item::Child(name) => write!(f, "child `{name}`"),
            Item::Text => f.write_str("text"),
        }
    }
}

/// A limit which a document exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Limit {
    /// [`MAX_NAME_LEN`](crate::MAX_NAME_LEN).
    NameLen,
    /// [`MAX_PREFIX_LEN`](crate::MAX_PREFIX_LEN).
    PrefixLen,
    /// [`MAX_NAMESPACE_LEN`](crate::MAX_NAMESPACE_LEN).
    NamespaceLen,
    /// [`MAX_NAMESPACES`](crate::MAX_NAMESPACES).
    Namespaces,
    /// The number of qualified names (`u32::MAX`).
    QualifiedNames,
    /// [`MAX_ATTRS`](crate::MAX_ATTRS).
    Attrs,
    /// [`MAX_CHILD_NAMES`](crate::MAX_CHILD_NAMES).
    ChildNames,
    /// The number of elements in a document (`u32::MAX + 1`).
    Elements,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::names::*;
        match self {
            Limit::NameLen => write!(f, "name longer than {MAX_NAME_LEN} bytes"),
            Limit::PrefixLen => write!(f, "prefix longer than {MAX_PREFIX_LEN} bytes"),
            Limit::NamespaceLen => write!(f, "namespace longer than {MAX_NAMESPACE_LEN} bytes"),
            Limit::Namespaces => write!(f, "more than {MAX_NAMESPACES} namespaces"),
            Limit::QualifiedNames => write!(f, "more than {} qualified names", u32::MAX),
            Limit::Attrs => write!(f, "element with more than {MAX_ATTRS} attributes"),
            Limit::ChildNames => write!(
                f,
                "element with children with more than {MAX_CHILD_NAMES} different names"
            ),
            Limit::Elements => write!(f, "more than {} elements", u64::from(u32::MAX) + 1),
        }
    }
}

/// The error type of the library.
///
/// The details are in [`ErrorKind`]. The error is boxed, so `Result<T, Error>` stays small.
///
/// ```
/// use use_xml::ErrorKind;
///
/// let error = use_xml::parse_str("<r>\n  <a><b/>text</a></r>").unwrap_err();
/// assert!(matches!(error.kind(), ErrorKind::MixedContent { .. }));
/// assert_eq!(error.path(), Some("/r/a[1]"));
/// assert_eq!(error.position(), Some(use_xml::Position { line: 2, column: 10 }));
/// ```
#[derive(Clone, PartialEq, Eq, thiserror::Error)]
#[error(transparent)]
pub struct Error(Box<ErrorKind>);

impl Error {
    /// Returns the details of the error.
    pub fn kind(&self) -> &ErrorKind {
        &self.0
    }

    /// Converts the error into its details.
    pub fn into_kind(self) -> ErrorKind {
        *self.0
    }

    /// Returns the position in the source document. See [`ErrorKind::position`].
    pub fn position(&self) -> Option<Position> {
        self.0.position()
    }

    /// Returns the path of the element related to the error. See [`ErrorKind::path`].
    pub fn path(&self) -> Option<&str> {
        self.0.path()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self(Box::new(kind))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show the details directly, without the wrapper.
        self.0.fmt(f)
    }
}

impl PartialEq<ErrorKind> for Error {
    fn eq(&self, other: &ErrorKind) -> bool {
        *self.0 == *other
    }
}

/// Details of an [`Error`].
///
/// Errors related to an element contain the path of the element
/// (e.g. `/root/items[1]/item[3]`) and a position in the source document.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The document is not well-formed XML.
    #[error("malformed XML at {position}: {message}")]
    Xml {
        /// Description of the problem.
        message: String,
        /// Where the problem was detected.
        position: Position,
    },

    /// The document is not namespace-well-formed, e.g. it uses an undeclared prefix
    /// or binds a reserved prefix or namespace.
    #[error("namespace error at {position}: {message}")]
    Namespace {
        /// Description of the problem.
        message: String,
        /// Position of the name or the declaration.
        position: Position,
    },

    /// The input is not valid UTF-8 or declares an unsupported encoding.
    #[error("unsupported encoding at {position}: {message}")]
    Encoding {
        /// Description of the problem.
        message: String,
        /// Where the problem was detected.
        position: Position,
    },

    /// A node which is not allowed at the given place,
    /// e.g. `DOCTYPE`, a second root element, or text outside of the root element.
    #[error("unexpected {node} at {position}")]
    UnexpectedNode {
        /// Description of the node.
        node: String,
        /// Position of the node.
        position: Position,
    },

    /// A limit was exceeded (see [`Limit`]).
    ///
    /// The position is `None` only for [`Document::namespace`](crate::Document::namespace).
    #[error("limit exceeded{}: {limit}", at(position))]
    LimitExceeded {
        /// The exceeded limit.
        limit: Limit,
        /// Where the limit was exceeded.
        position: Option<Position>,
    },

    /// A reference to an entity which is neither predefined nor a character reference.
    #[error("unknown entity `{name}` at {position}")]
    UnknownEntity {
        /// Name of the entity.
        name: String,
        /// Position of the reference (also inside an attribute value).
        position: Position,
    },

    /// An element contains both child elements and significant text.
    #[error("element {path} contains mixed content at {position}")]
    MixedContent {
        /// Path of the element.
        path: String,
        /// Position of the significant text: its first non-whitespace character,
        /// the start of the CDATA section, or `&` of the reference.
        position: Position,
    },

    /// An element has two attributes with the same name as written
    /// or with the same namespace and local name.
    #[error("element {path} contains duplicate attribute `{name}` at {position}")]
    DuplicateAttr {
        /// Path of the element.
        path: String,
        /// Position of the second attribute.
        position: Position,
        /// Name of the second attribute as written in the document.
        name: String,
    },

    /// Elements are nested deeper than [`MAX_DEPTH`](crate::MAX_DEPTH).
    #[error(
        "element {path} contains a child at {position} which exceeds the maximum depth {max_depth}"
    )]
    MaxDepthExceeded {
        /// Path of the deepest allowed element.
        path: String,
        /// Position of the child which is too deep.
        position: Position,
        /// The maximum allowed depth.
        max_depth: usize,
    },

    /// A [`NamespaceHandle`](crate::NamespaceHandle) which does not belong to the document.
    #[error("element {path} at {position} received a namespace handle of another document")]
    InvalidNamespaceHandle {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
    },

    /// [`Document::namespace`](crate::Document::namespace) received a namespace name
    /// which cannot occur in a document.
    #[error("invalid namespace `{namespace}`: {message}")]
    InvalidNamespace {
        /// The namespace name, cut to [`MAX_VALUE_CHARS`] characters.
        namespace: String,
        /// Description of the problem.
        message: String,
    },

    /// An attribute or children were requested by a local name
    /// which is not an `NCName` or is longer than [`MAX_NAME_LEN`](crate::MAX_NAME_LEN).
    #[error("invalid {item} requested from element {path} at {position}: {message}")]
    InvalidName {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// The requested item; its local name is cut to [`MAX_VALUE_CHARS`] characters.
        item: Item,
        /// Description of the problem.
        message: String,
    },

    /// A required attribute or child is missing.
    #[error("element {path} at {position} has no {item}")]
    Missing {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// What is missing.
        item: Item,
    },

    /// An unexpected number of children with the given qualified name.
    #[error("element {path} at {position} has {found} children `{name}`, expected {expected}")]
    WrongCount {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// Qualified name of the children.
        name: QualifiedName,
        /// Description of the expected count, e.g. `exactly one`.
        expected: &'static str,
        /// The actual count.
        found: usize,
    },

    /// An item was requested more than once.
    #[error("{item} of element {path} at {position} already used")]
    AlreadyUsed {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// The item which was already used.
        item: Item,
    },

    /// A value parser rejected an attribute value or text.
    ///
    /// The value itself is not stored because it was moved into the value parser.
    /// The parsers from the [`parse`](crate::parse) module include it in the message.
    #[error("invalid {item} of element {path} at {position}: {message}")]
    InvalidValue {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// The attribute or text.
        item: Item,
        /// Message from the value parser.
        message: String,
    },

    /// An item was not used, reported by [`Elem::check_all_used`](crate::Elem::check_all_used).
    ///
    /// For an unused child name ([`Item::Child`]) the path and the position are those
    /// of the first child with this name, not of the parent.
    #[error("{}", unused_message(item, path, position))]
    Unused {
        /// Path of the element whose attribute or text was not used,
        /// or path of the first unused child.
        path: String,
        /// Position of the element whose attribute or text was not used,
        /// or position of the first unused child.
        position: Position,
        /// The item which was not used.
        item: Item,
    },
}

fn at(position: &Option<Position>) -> String {
    match position {
        Some(position) => format!(" at {position}"),
        None => String::new(),
    }
}

fn unused_message(item: &Item, path: &str, position: &Position) -> String {
    match item {
        Item::Child(_) => format!("element {path} at {position} not used"),
        _ => format!("{item} of element {path} at {position} not used"),
    }
}

impl ErrorKind {
    /// Returns the position in the source document.
    ///
    /// It is the position of the problem for [`Xml`](Self::Xml), [`Namespace`](Self::Namespace),
    /// [`Encoding`](Self::Encoding), [`UnexpectedNode`](Self::UnexpectedNode),
    /// [`LimitExceeded`](Self::LimitExceeded), and [`UnknownEntity`](Self::UnknownEntity),
    /// the position of the offending attribute or child for [`DuplicateAttr`](Self::DuplicateAttr)
    /// and [`MaxDepthExceeded`](Self::MaxDepthExceeded), the position of the significant text
    /// for [`MixedContent`](Self::MixedContent), and otherwise the position
    /// of the element given by [`path`](Self::path).
    ///
    /// Returns `None` only for [`LimitExceeded`](Self::LimitExceeded)
    /// from [`Document::namespace`](crate::Document::namespace)
    /// and for [`InvalidNamespace`](Self::InvalidNamespace).
    pub fn position(&self) -> Option<Position> {
        match self {
            Self::LimitExceeded { position, .. } => *position,
            Self::InvalidNamespace { .. } => None,
            Self::Xml { position, .. }
            | Self::Namespace { position, .. }
            | Self::Encoding { position, .. }
            | Self::UnexpectedNode { position, .. }
            | Self::UnknownEntity { position, .. }
            | Self::MixedContent { position, .. }
            | Self::DuplicateAttr { position, .. }
            | Self::MaxDepthExceeded { position, .. }
            | Self::InvalidNamespaceHandle { position, .. }
            | Self::InvalidName { position, .. }
            | Self::Missing { position, .. }
            | Self::WrongCount { position, .. }
            | Self::AlreadyUsed { position, .. }
            | Self::InvalidValue { position, .. }
            | Self::Unused { position, .. } => Some(*position),
        }
    }

    /// Returns the path of the element related to the error, e.g. `/root/items[1]/item[3]`.
    ///
    /// Returns `None` for [`Xml`](Self::Xml), [`Namespace`](Self::Namespace),
    /// [`Encoding`](Self::Encoding), [`UnexpectedNode`](Self::UnexpectedNode),
    /// [`LimitExceeded`](Self::LimitExceeded), [`UnknownEntity`](Self::UnknownEntity),
    /// and [`InvalidNamespace`](Self::InvalidNamespace), which are not related to a complete element.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Xml { .. }
            | Self::Namespace { .. }
            | Self::Encoding { .. }
            | Self::UnexpectedNode { .. }
            | Self::LimitExceeded { .. }
            | Self::UnknownEntity { .. }
            | Self::InvalidNamespace { .. } => None,
            Self::MixedContent { path, .. }
            | Self::DuplicateAttr { path, .. }
            | Self::MaxDepthExceeded { path, .. }
            | Self::InvalidNamespaceHandle { path, .. }
            | Self::InvalidName { path, .. }
            | Self::Missing { path, .. }
            | Self::WrongCount { path, .. }
            | Self::AlreadyUsed { path, .. }
            | Self::InvalidValue { path, .. }
            | Self::Unused { path, .. } => Some(path),
        }
    }
}
