//! Extract data from XML documents and check that nothing was left unused.
//!
//! A document is parsed by [`parse_str`] or [`parse_bytes`] into a [`Document`].
//! Its elements are accessed through [`Elem`] values, starting with [`Document::root`].
//! Elements and attributes are identified by a namespace ([`NamespaceHandle`],
//! obtained by [`Document::namespace`]) and a local name.
//! Attributes and texts are converted by value parsers from the [`parse`] module
//! (or any function `FnOnce(String) -> Result<T, E>`). Every item can be used only once.
//! At the end [`Document::check_all_used`] reports the first item which was not used.
//!
//! ```
//! use use_xml::parse;
//!
//! let xml = r#"
//! <people xmlns="urn:people">
//!     <person name="Pete" />
//!     <person name="Harry" address="Little Whinging" />
//! </people>
//! "#;
//! let doc = use_xml::parse_str(xml)?;
//! let people_ns = doc.namespace("urn:people")?;
//! let no_ns = doc.namespace("")?;
//! let people = doc
//!     .root()
//!     .children(people_ns, "person")?
//!     .into_iter()
//!     .map(|p| Ok((p.attr(no_ns, "name", parse::string)?, p.attr_opt(no_ns, "address", parse::string)?)))
//!     .collect::<Result<Vec<_>, use_xml::Error>>()?;
//! doc.check_all_used()?;
//!
//! assert_eq!(people, [
//!     ("Pete".to_owned(), None),
//!     ("Harry".to_owned(), Some("Little Whinging".to_owned())),
//! ]);
//! # Ok::<(), use_xml::Error>(())
//! ```
#![forbid(unsafe_code)]

mod document;
mod elem;
mod error;
mod names;
pub mod parse;
mod parser;

pub use document::{Document, MAX_DEPTH, parse_bytes, parse_str};
pub use elem::Elem;
pub use error::{Error, ErrorKind, Item, Limit, MAX_VALUE_CHARS, Position};
pub use names::{
    MAX_ATTRS, MAX_CHILD_NAMES, MAX_NAME_LEN, MAX_NAMESPACE_LEN, MAX_NAMESPACES, MAX_PREFIX_LEN,
    NamespaceHandle, QualifiedName,
};

/// `Document` uses `Cell` and `RefCell` for usage tracking, so it must not be `Sync`.
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<use_xml::Document>();
/// ```
#[cfg(doctest)]
struct DocumentIsNotSync;

/// An `Elem` must not outlive its document.
///
/// ```compile_fail
/// let root = {
///     let doc = use_xml::parse_str("<a/>").unwrap();
///     doc.root()
/// };
/// root.local_name();
/// ```
#[cfg(doctest)]
struct ElemDoesNotOutliveDocument;

// Compiles the examples in the READMEs as doc tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../README.cs.md")]
struct ReadmeCsDoctests;
