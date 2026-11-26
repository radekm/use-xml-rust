//! The error type of the library.

use std::borrow::Cow;
use std::fmt;

/// Maximum number of characters of a value stored in [`ErrorKind::InvalidValue`]
/// and shown by the `Debug` implementation of [`Elem`](crate::Elem).
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
    /// An attribute with the given local name.
    Attr(String),
    /// Children with the given local name.
    Child(String),
    /// The text content of the element.
    Text,
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
/// assert_eq!(error.position(), use_xml::Position { line: 2, column: 3 });
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
    pub fn position(&self) -> Position {
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
    ///
    /// An invalid reference in an attribute value points to the `&` of the reference.
    #[error("malformed XML at {position}: {message}")]
    Xml {
        /// Description of the problem.
        message: String,
        /// Where the problem was detected.
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
    /// e.g. `DOCTYPE` or text outside of the root element.
    #[error("unexpected {node} at {position}")]
    UnexpectedNode {
        /// Description of the node.
        node: String,
        /// Position of the node.
        position: Position,
    },

    /// A reference to an entity which is neither predefined nor a character reference.
    #[error("unknown entity `{name}` in element {path} at {position}")]
    UnknownEntity {
        /// Name of the entity.
        name: String,
        /// Path of the element containing the reference.
        path: String,
        /// Position of the reference (also inside an attribute value).
        position: Position,
    },

    /// An element contains both child elements and significant text.
    #[error("element {path} at {position} contains mixed content")]
    MixedContent {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
    },

    /// An element has multiple attributes with the same local name.
    ///
    /// Prefixes are ignored, so e.g. `xml:lang` and `lang` on one element are duplicates too.
    /// Namespace declarations (`xmlns`, `xmlns:*`) are skipped and never reported.
    #[error("element {path} at {position} contains multiple attributes with local name `{name}`")]
    DuplicateAttr {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// Local name of the attribute.
        name: String,
    },

    /// An element has children with the same local name but different prefixes.
    #[error(
        "element {path} contains children with local name `{name}` \
         but different prefixes `{first_prefix}` and `{second_prefix}` (second one at {position})"
    )]
    ChildPrefixMismatch {
        /// Path of the parent element.
        path: String,
        /// Position of the child whose prefix differs.
        position: Position,
        /// Local name of the children.
        name: String,
        /// Prefix of the first child with the local name (empty if none).
        first_prefix: String,
        /// Prefix of the child which differs (empty if none).
        second_prefix: String,
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

    /// An unexpected number of children with the given local name.
    #[error("element {path} at {position} has {found} children `{name}`, expected {expected}")]
    WrongCount {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// Local name of the children.
        name: String,
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
    #[error("invalid {item} of element {path} at {position}: {message} (value: {value:?})")]
    InvalidValue {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// The attribute or text.
        item: Item,
        /// The rejected value. Values longer than [`MAX_VALUE_CHARS`] characters
        /// are cut and end with `…`, so a huge text does not end up in the error.
        value: String,
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

fn unused_message(item: &Item, path: &str, position: &Position) -> String {
    match item {
        Item::Child(_) => format!("element {path} at {position} not used"),
        _ => format!("{item} of element {path} at {position} not used"),
    }
}

impl ErrorKind {
    /// Returns the position in the source document.
    ///
    /// It is the position of the problem for [`Xml`](Self::Xml), [`Encoding`](Self::Encoding),
    /// [`UnexpectedNode`](Self::UnexpectedNode), and [`UnknownEntity`](Self::UnknownEntity),
    /// the position of the offending child for [`ChildPrefixMismatch`](Self::ChildPrefixMismatch)
    /// and [`MaxDepthExceeded`](Self::MaxDepthExceeded), and otherwise the position
    /// of the element given by [`path`](Self::path).
    pub fn position(&self) -> Position {
        match self {
            Self::Xml { position, .. }
            | Self::Encoding { position, .. }
            | Self::UnexpectedNode { position, .. }
            | Self::UnknownEntity { position, .. }
            | Self::MixedContent { position, .. }
            | Self::DuplicateAttr { position, .. }
            | Self::ChildPrefixMismatch { position, .. }
            | Self::MaxDepthExceeded { position, .. }
            | Self::Missing { position, .. }
            | Self::WrongCount { position, .. }
            | Self::AlreadyUsed { position, .. }
            | Self::InvalidValue { position, .. }
            | Self::Unused { position, .. } => *position,
        }
    }

    /// Returns the path of the element related to the error, e.g. `/root/items[1]/item[3]`.
    ///
    /// Returns `None` for [`Xml`](Self::Xml), [`Encoding`](Self::Encoding),
    /// and [`UnexpectedNode`](Self::UnexpectedNode), which are not related to an element.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Xml { .. } | Self::Encoding { .. } | Self::UnexpectedNode { .. } => None,
            Self::UnknownEntity { path, .. }
            | Self::MixedContent { path, .. }
            | Self::DuplicateAttr { path, .. }
            | Self::ChildPrefixMismatch { path, .. }
            | Self::MaxDepthExceeded { path, .. }
            | Self::Missing { path, .. }
            | Self::WrongCount { path, .. }
            | Self::AlreadyUsed { path, .. }
            | Self::InvalidValue { path, .. }
            | Self::Unused { path, .. } => Some(path),
        }
    }
}
