//! The element tree and the extraction API.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use crate::error::{Error, ErrorKind, Item, Position};

/// Path of an element, shared between the element and its descendants.
#[derive(Debug)]
pub(crate) struct ElemPath {
    pub(crate) parent: Option<Arc<ElemPath>>,
    /// Local name of the element.
    pub(crate) name: String,
    /// Prefix of the element, empty if there is none.
    pub(crate) prefix: String,
    /// Zero-based index among siblings with the same local name.
    pub(crate) index: usize,
}

impl fmt::Display for ElemPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Collect the ancestors iteratively to avoid deep recursion.
        let mut nodes = Vec::new();
        let mut node = Some(self);
        while let Some(n) = node {
            nodes.push(n);
            node = n.parent.as_deref();
        }
        for n in nodes.iter().rev() {
            if n.parent.is_none() {
                write!(f, "/{}", n.name)?;
            } else {
                write!(f, "/{}[{}]", n.name, n.index + 1)?;
            }
        }
        Ok(())
    }
}

/// An XML element with usage tracking.
///
/// Every attribute, every child name, and the text can be used at most once.
/// After the extraction call [`Elem::check_all_used`] to make sure that nothing was left unused.
#[derive(Debug)]
pub struct Elem {
    pub(crate) path: Arc<ElemPath>,
    pub(crate) position: Position,
    /// Local name -> (used, value).
    pub(crate) attrs: BTreeMap<String, (Cell<bool>, String)>,
    /// Local name -> (used, children in document order).
    pub(crate) children: BTreeMap<String, (Cell<bool>, Vec<Elem>)>,
    /// Requested attributes and children which do not exist.
    /// Allocated only on the first such request because it is rarely needed.
    #[expect(
        clippy::box_collection,
        reason = "the set is rarely needed, `Box` keeps `Elem` smaller than an empty `BTreeSet`"
    )]
    pub(crate) missing_used: RefCell<Option<Box<BTreeSet<Item>>>>,
    pub(crate) text: Box<str>,
    /// The text contains a non-whitespace character or a CDATA section.
    pub(crate) text_significant: bool,
    pub(crate) text_used: Cell<bool>,
    /// `ignore_all` was called: everything in the element, including its descendants, is used
    /// and any further request fails with already used.
    pub(crate) all_used: Cell<bool>,
}

impl Elem {
    /// Local name of the element (without a prefix).
    pub fn name(&self) -> &str {
        &self.path.name
    }

    /// Path of the element, e.g. `/root/items[1]/item[3]`.
    ///
    /// The root has no index; other elements have a 1-based index
    /// among siblings with the same local name.
    pub fn path(&self) -> String {
        self.path.to_string()
    }

    /// Position of the start tag of the element in the source document.
    pub fn position(&self) -> Position {
        self.position
    }

    fn error_already_used(&self, item: Item) -> Error {
        ErrorKind::AlreadyUsed {
            path: self.path(),
            position: self.position,
            item,
        }
        .into()
    }

    /// Looks up `name` in `map` and marks it as used.
    /// Names which are not in `map` are recorded in `missing_used`.
    /// Fails if the name was already used.
    fn use_entry<'s, V>(
        &self,
        map: &'s BTreeMap<String, (Cell<bool>, V)>,
        name: &str,
        item: impl Fn() -> Item,
    ) -> Result<Option<&'s V>, Error> {
        let already_used = self.all_used.get()
            || match map.get(name) {
                Some((used, value)) => {
                    if !used.replace(true) {
                        return Ok(Some(value));
                    }
                    true
                }
                None => {
                    // The borrow ends before any user code (e.g. a value parser) runs.
                    let mut missing_used = self.missing_used.borrow_mut();
                    !missing_used.get_or_insert_default().insert(item())
                }
            };
        if already_used {
            Err(self.error_already_used(item()))
        } else {
            Ok(None)
        }
    }

    fn run_parser<T, E: fmt::Display>(
        &self,
        item: impl FnOnce() -> Item,
        value: &str,
        parser: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, Error> {
        parser(value).map_err(|e| {
            ErrorKind::InvalidValue {
                path: self.path(),
                position: self.position,
                item: item(),
                value: value.to_owned(),
                message: e.to_string(),
            }
            .into()
        })
    }

    /// Returns the attribute `name` converted by `parser`, or `None` if the attribute is missing.
    ///
    /// The attribute is marked as used even if it is missing.
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if the attribute was already requested.
    /// - [`ErrorKind::InvalidValue`] if `parser` fails.
    pub fn attr_opt<T, E: fmt::Display>(
        &self,
        name: &str,
        parser: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<Option<T>, Error> {
        let item = || Item::Attr(name.to_owned());
        match self.use_entry(&self.attrs, name, item)? {
            None => Ok(None),
            Some(value) => self.run_parser(item, value, parser).map(Some),
        }
    }

    /// Returns the attribute `name` converted by `parser`.
    ///
    /// ```
    /// let root = use_xml::parse_str(r#"<person name="Pete" age="42" />"#)?;
    /// assert_eq!(root.attr("name", use_xml::parse::string)?, "Pete");
    /// assert_eq!(root.attr("age", use_xml::parse::u32)?, 42);
    /// root.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::Missing`] if the attribute is missing.
    /// - [`ErrorKind::AlreadyUsed`] if the attribute was already requested.
    /// - [`ErrorKind::InvalidValue`] if `parser` fails.
    pub fn attr<T, E: fmt::Display>(
        &self,
        name: &str,
        parser: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, Error> {
        self.attr_opt(name, parser)?.ok_or_else(|| {
            ErrorKind::Missing {
                path: self.path(),
                position: self.position,
                item: Item::Attr(name.to_owned()),
            }
            .into()
        })
    }

    /// Returns the text of the element converted by `parser`.
    ///
    /// An element with child elements or without content has the text `""`.
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if the text was already requested.
    /// - [`ErrorKind::InvalidValue`] if `parser` fails.
    pub fn text<T, E: fmt::Display>(
        &self,
        parser: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, Error> {
        if self.all_used.get() || self.text_used.replace(true) {
            return Err(self.error_already_used(Item::Text));
        }
        self.run_parser(|| Item::Text, &self.text, parser)
    }

    /// Returns all children with the local name `name` in document order.
    ///
    /// The name is marked as used even if there are no such children.
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if children with this name were already requested.
    pub fn children(&self, name: &str) -> Result<&[Elem], Error> {
        let item = || Item::Child(name.to_owned());
        let children = self.use_entry(&self.children, name, item)?;
        Ok(children.map_or(&[], Vec::as_slice))
    }

    /// Returns the child with the local name `name`, or `None` if there is no such child.
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::WrongCount`] if there are multiple such children.
    /// - [`ErrorKind::AlreadyUsed`] if children with this name were already requested.
    pub fn child_opt(&self, name: &str) -> Result<Option<&Elem>, Error> {
        match self.children(name)? {
            [] => Ok(None),
            [child] => Ok(Some(child)),
            children => Err(ErrorKind::WrongCount {
                path: self.path(),
                position: self.position,
                name: name.to_owned(),
                expected: "at most one",
                found: children.len(),
            }
            .into()),
        }
    }

    /// Returns the only child with the local name `name`.
    ///
    /// ```
    /// let root = use_xml::parse_str("<animals><pig>B</pig></animals>")?;
    /// let pig = root.child("pig")?;
    /// assert_eq!(pig.text(use_xml::parse::string)?, "B");
    /// root.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::Missing`] if there is no such child.
    /// - [`ErrorKind::WrongCount`] if there are multiple such children.
    /// - [`ErrorKind::AlreadyUsed`] if children with this name were already requested.
    pub fn child(&self, name: &str) -> Result<&Elem, Error> {
        match self.children(name)? {
            [] => Err(ErrorKind::Missing {
                path: self.path(),
                position: self.position,
                item: Item::Child(name.to_owned()),
            }
            .into()),
            [child] => Ok(child),
            children => Err(ErrorKind::WrongCount {
                path: self.path(),
                position: self.position,
                name: name.to_owned(),
                expected: "exactly one",
                found: children.len(),
            }
            .into()),
        }
    }

    /// Marks the element and all its descendants as used.
    ///
    /// Any later request on the element or its descendants fails with [`ErrorKind::AlreadyUsed`].
    pub fn ignore_all(&self) {
        let mut stack = vec![self];
        while let Some(elem) = stack.pop() {
            if !elem.all_used.replace(true) {
                stack.extend(elem.children.values().flat_map(|(_, elems)| elems));
            }
        }
    }

    /// Checks that everything in the element and its descendants was used.
    ///
    /// Returns [`ErrorKind::Unused`] for the first unused item. Within an element the order is:
    /// attributes (by name), significant text, child names (by name), and then descendants
    /// of the used child names (by name, then in document order).
    /// Text consisting only of whitespace does not need to be used.
    pub fn check_all_used(&self) -> Result<(), Error> {
        if self.all_used.get() {
            return Ok(());
        }
        let unused = |item| {
            ErrorKind::Unused {
                path: self.path(),
                position: self.position,
                item,
            }
            .into()
        };

        if let Some((name, _)) = self.attrs.iter().find(|(_, (used, _))| !used.get()) {
            return Err(unused(Item::Attr(name.clone())));
        }
        if self.text_significant && !self.text_used.get() {
            return Err(unused(Item::Text));
        }
        if let Some((name, _)) = self.children.iter().find(|(_, (used, _))| !used.get()) {
            return Err(unused(Item::Child(name.clone())));
        }
        // Recursion depth is bounded by `MAX_DEPTH`.
        self.children
            .values()
            .flat_map(|(_, elems)| elems)
            .try_for_each(Elem::check_all_used)
    }
}
