//! The error type of the library.

use std::fmt;

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

/// Every error which can be returned by the library.
///
/// Errors related to an element contain the path of the element
/// (e.g. `/root/items[1]/item[3]`) and a position in the source document.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The document is not well-formed XML.
    #[error("malformed XML at {position}: {message}")]
    Xml {
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
        /// Position of the reference.
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
        /// The rejected value.
        value: String,
        /// Message from the value parser.
        message: String,
    },

    /// An item was not used, reported by [`Elem::check_all_used`](crate::Elem::check_all_used).
    #[error("{item} of element {path} at {position} not used")]
    Unused {
        /// Path of the element.
        path: String,
        /// Position of the element.
        position: Position,
        /// The item which was not used.
        item: Item,
    },
}
