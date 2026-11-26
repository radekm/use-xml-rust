//! Extract data from XML documents and check that nothing was left unused.
//!
//! The document is parsed by [`parse_str`] or [`parse_bytes`] into a tree of [`Elem`] values.
//! Attributes, children, and text are requested with methods of [`Elem`]
//! and converted by value parsers from the [`parse`] module (or any function
//! `FnOnce(&str) -> Result<T, E>`). Every item can be used only once.
//! At the end [`Elem::check_all_used`] reports the first item which was not used.
//!
//! ```
//! use use_xml::parse;
//!
//! let xml = r#"
//! <people>
//!     <person name="Pete" />
//!     <person name="Harry" address="Little Whinging" />
//! </people>
//! "#;
//! let root = use_xml::parse_str(xml)?;
//! let people = root
//!     .children("person")?
//!     .iter()
//!     .map(|p| Ok((p.attr("name", parse::string)?, p.attr_opt("address", parse::string)?)))
//!     .collect::<Result<Vec<_>, use_xml::Error>>()?;
//! root.check_all_used()?;
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
pub mod parse;

pub use document::{MAX_DEPTH, parse_bytes, parse_str};
pub use elem::Elem;
pub use error::{Error, ErrorKind, Item, MAX_VALUE_CHARS, Position};

/// `Elem` uses `Cell` and `RefCell` for usage tracking, so it must not be `Sync`.
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<use_xml::Elem>();
/// ```
#[cfg(doctest)]
struct ElemIsNotSync;

// Compiles the examples in the READMEs as doc tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../README.cs.md")]
struct ReadmeCsDoctests;
