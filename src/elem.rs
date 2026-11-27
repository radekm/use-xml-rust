//! Elements and the extraction API.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use crate::document::{Document, Node};
use crate::error::{Error, ErrorKind, Item, Position, truncate_value};
use crate::names::{MAX_NAME_LEN, NamespaceHandle, QualifiedName, QualifiedNameHandle};
use crate::parser::is_ncname;

/// An element of a [`Document`].
///
/// `Elem` is a small `Copy` handle which borrows the document, so it cannot outlive it.
/// All methods take `self` by value; usage of attributes, texts, and children
/// is tracked inside the document.
///
/// Each attribute, each child name, and the text can be requested only once.
/// A second request fails with [`ErrorKind::AlreadyUsed`].
/// At the end [`Elem::check_all_used`] (or [`Document::check_all_used`])
/// reports the first item which was not requested.
///
/// Requests for missing attributes and children are recorded too, so their names
/// are interned in the document. This is not limited by [`MAX_ATTRS`](crate::MAX_ATTRS)
/// or [`MAX_CHILD_NAMES`](crate::MAX_CHILD_NAMES), which apply only to the parsed document.
#[derive(Clone, Copy)]
pub struct Elem<'doc> {
    doc: &'doc Document,
    index: u32,
}

impl fmt::Debug for Elem<'_> {
    /// Prints only this element, not its descendants: its path, position, attributes,
    /// the number of children for each name, and its text.
    /// Values longer than [`MAX_VALUE_CHARS`](crate::MAX_VALUE_CHARS) characters are cut and end with `…`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// A value which is already formatted.
        struct Raw(String);
        impl fmt::Debug for Raw {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        let value = |value: Option<&str>| {
            Raw(value.map_or_else(
                || "<used>".to_owned(),
                |value| format!("{:?}", truncate_value(value)),
            ))
        };

        // Everything is collected first, so no `RefCell` is borrowed while `f` writes.
        let node = self.node();
        let names = &self.doc.names;
        let attrs: BTreeMap<String, Raw> = node
            .attrs
            .borrow()
            .iter()
            .map(|(name, v)| (names.qualified_name(*name).to_string(), value(v.as_deref())))
            .collect();
        let children: BTreeMap<String, usize> = node
            .children
            .borrow()
            .iter()
            .map(|(name, (_, elems))| (names.qualified_name(*name).to_string(), elems.len()))
            .collect();
        let text = node.text.take();
        let text_debug = value(text.as_deref());
        node.text.set(text);

        f.debug_struct("Elem")
            .field("path", &self.path())
            .field("position", &node.position)
            .field("attrs", &attrs)
            .field("children", &children)
            .field("text", &text_debug)
            .finish()
    }
}

impl<'doc> Elem<'doc> {
    pub(crate) fn new(doc: &'doc Document, index: u32) -> Self {
        Self { doc, index }
    }

    fn node(&self) -> &'doc Node {
        self.doc.node(self.index)
    }

    /// The document of the element.
    pub fn document(self) -> &'doc Document {
        self.doc
    }

    /// The parent element, `None` for the root.
    pub fn parent(self) -> Option<Elem<'doc>> {
        self.node().parent.map(|p| Elem::new(self.doc, p))
    }

    /// The local name of the element (without a prefix).
    pub fn local_name(self) -> &'doc str {
        self.doc.names.frozen_name(self.node().name).1
    }

    /// The namespace of the element.
    pub fn namespace(self) -> NamespaceHandle {
        self.doc.names.frozen_name(self.node().name).0
    }

    /// The namespace name (URI) of the element; empty if the element is in no namespace.
    pub fn namespace_uri(self) -> &'doc str {
        self.doc.names.frozen_namespace_uri(self.namespace())
    }

    /// The name of the element as written in the document, including the prefix.
    pub fn prefixed_name(self) -> &'doc str {
        self.doc.names.raw_name(self.node().raw_name)
    }

    /// Path of the element, e.g. `/root/items[1]/item[3]`.
    ///
    /// Names are written as in the document (with prefixes). The root has no index;
    /// other elements have a 1-based index among siblings with the same qualified name.
    pub fn path(self) -> String {
        self.doc.path(self.index)
    }

    /// Position of the start tag of the element in the source document.
    pub fn position(self) -> Position {
        self.node().position
    }

    fn error_already_used(self, item: Item) -> Error {
        ErrorKind::AlreadyUsed {
            path: self.path(),
            position: self.position(),
            item,
        }
        .into()
    }

    fn qualified_name(self, ns: NamespaceHandle, local: &str) -> QualifiedName {
        QualifiedName {
            namespace: self.doc.names.namespace_uri(ns),
            local_name: local.to_owned(),
        }
    }

    /// Checks the requested name and finds it or interns it (for recording a missing name).
    fn name_handle(
        self,
        ns: NamespaceHandle,
        local: &str,
        item: fn(QualifiedName) -> Item,
    ) -> Result<QualifiedNameHandle, Error> {
        if !self.doc.names.is_valid_namespace(ns) {
            return Err(ErrorKind::InvalidNamespaceHandle {
                path: self.path(),
                position: self.position(),
            }
            .into());
        }
        let message = if local.len() > MAX_NAME_LEN {
            Some(format!("local name longer than {MAX_NAME_LEN} bytes"))
        } else if !is_ncname(local) {
            Some("local name is not an NCName (a name without a colon)".to_owned())
        } else {
            None
        };
        if let Some(message) = message {
            let name = QualifiedName {
                namespace: self.doc.names.namespace_uri(ns),
                local_name: truncate_value(local).into_owned(),
            };
            return Err(ErrorKind::InvalidName {
                path: self.path(),
                position: self.position(),
                item: item(name),
                message,
            }
            .into());
        }
        self.doc.names.intern_name(ns, local).map_err(|limit| {
            ErrorKind::LimitExceeded {
                limit,
                position: Some(self.position()),
            }
            .into()
        })
    }

    fn run_parser<T, E: fmt::Display>(
        self,
        item: impl FnOnce() -> Item,
        value: String,
        parser: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, Error> {
        parser(value).map_err(|e| {
            ErrorKind::InvalidValue {
                path: self.path(),
                position: self.position(),
                item: item(),
                message: e.to_string(),
            }
            .into()
        })
    }

    /// Returns the attribute converted by `parser`, or `None` if the attribute is missing.
    ///
    /// The attribute is marked as used even if it is missing. Unprefixed attributes
    /// are in no namespace ([`NamespaceHandle::NO_NAMESPACE`]), even if there is
    /// a default namespace declaration.
    ///
    /// ```
    /// use use_xml::{ErrorKind, Item, NamespaceHandle, parse};
    ///
    /// let doc = use_xml::parse_str(r#"<person xmlns="urn:p" name="Pete" />"#)?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// let root = doc.root();
    /// assert_eq!(root.attr_opt(ns, "name", parse::string)?, Some("Pete".to_owned()));
    /// assert_eq!(root.attr_opt(ns, "age", parse::u32)?, None);
    /// // The missing attribute was marked as used.
    /// assert!(matches!(
    ///     root.attr_opt(ns, "age", parse::u32).unwrap_err().kind(),
    ///     ErrorKind::AlreadyUsed { item: Item::Attr(_), .. }
    /// ));
    /// doc.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if the attribute was already requested.
    /// - [`ErrorKind::InvalidValue`] if `parser` fails.
    /// - [`ErrorKind::InvalidNamespaceHandle`] if `ns` is out of range of the document
    ///   (a handle of another document is not always detected, see [`NamespaceHandle`]).
    /// - [`ErrorKind::InvalidName`] if `local_name` is not an `NCName`
    ///   or is longer than [`MAX_NAME_LEN`](crate::MAX_NAME_LEN).
    /// - [`ErrorKind::LimitExceeded`] with [`Limit::QualifiedNames`](crate::Limit::QualifiedNames)
    ///   if the name of a missing attribute cannot be interned.
    pub fn attr_opt<T, E: fmt::Display>(
        self,
        ns: NamespaceHandle,
        local_name: &str,
        parser: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<Option<T>, Error> {
        let item = || Item::Attr(self.qualified_name(ns, local_name));
        let node = self.node();
        let name = self.name_handle(ns, local_name, Item::Attr)?;
        if node.all_used.get() {
            return Err(self.error_already_used(item()));
        }
        // The borrow ends with the statement, before the value parser runs.
        let value = match node.attrs.borrow_mut().entry(name) {
            // A missing attribute is recorded as used.
            Entry::Vacant(entry) => {
                entry.insert(None);
                return Ok(None);
            }
            Entry::Occupied(mut entry) => entry.get_mut().take(),
        };
        let Some(value) = value else {
            return Err(self.error_already_used(item()));
        };
        self.run_parser(item, value.into_string(), parser).map(Some)
    }

    /// Returns the attribute converted by `parser`.
    ///
    /// ```
    /// use use_xml::{NamespaceHandle, parse};
    ///
    /// let doc = use_xml::parse_str(r#"<person name="Pete" age="42" />"#)?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// assert_eq!(doc.root().attr(ns, "name", parse::string)?, "Pete");
    /// assert_eq!(doc.root().attr(ns, "age", parse::u32)?, 42);
    /// doc.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::Missing`] if the attribute is missing.
    /// - The errors of [`Elem::attr_opt`].
    pub fn attr<T, E: fmt::Display>(
        self,
        ns: NamespaceHandle,
        local_name: &str,
        parser: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, Error> {
        self.attr_opt(ns, local_name, parser)?.ok_or_else(|| {
            ErrorKind::Missing {
                path: self.path(),
                position: self.position(),
                item: Item::Attr(self.qualified_name(ns, local_name)),
            }
            .into()
        })
    }

    /// Returns the text of the element converted by `parser`.
    ///
    /// An element with child elements or without content has the text `""`.
    /// Line endings are normalized, references are decoded, and CDATA sections
    /// are joined with the surrounding text.
    ///
    /// ```
    /// use use_xml::{NamespaceHandle, parse};
    ///
    /// let doc = use_xml::parse_str("<r><price>12&#46;50</price><note/></r>")?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// let price = doc.root().child(ns, "price")?.text(parse::decimal)?;
    /// assert_eq!(price.to_string(), "12.50");
    /// assert_eq!(doc.root().child(ns, "note")?.text(parse::string)?, "");
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if the text was already requested.
    /// - [`ErrorKind::InvalidValue`] if `parser` fails.
    pub fn text<T, E: fmt::Display>(
        self,
        parser: impl FnOnce(String) -> Result<T, E>,
    ) -> Result<T, Error> {
        let node = self.node();
        let text = if node.all_used.get() {
            None
        } else {
            node.text.take()
        };
        match text {
            None => Err(self.error_already_used(Item::Text)),
            Some(text) => self.run_parser(|| Item::Text, text.into_string(), parser),
        }
    }

    /// Marks the child name as used and calls `f` with the indices of the children.
    fn use_children<R>(
        self,
        ns: NamespaceHandle,
        local_name: &str,
        f: impl FnOnce(&[u32]) -> R,
    ) -> Result<R, Error> {
        let node = self.node();
        let name = self.name_handle(ns, local_name, Item::Child)?;
        let already_used =
            || self.error_already_used(Item::Child(self.qualified_name(ns, local_name)));
        if node.all_used.get() {
            return Err(already_used());
        }
        let mut children = node.children.borrow_mut();
        // A missing name is recorded with no children and then marked as used like any other.
        let (used, elems) = children.entry(name).or_default();
        if *used {
            drop(children);
            return Err(already_used());
        }
        *used = true;
        Ok(f(elems))
    }

    fn wrong_count(
        self,
        ns: NamespaceHandle,
        local_name: &str,
        expected: &'static str,
        found: usize,
    ) -> Error {
        ErrorKind::WrongCount {
            path: self.path(),
            position: self.position(),
            name: self.qualified_name(ns, local_name),
            expected,
            found,
        }
        .into()
    }

    /// Returns all children with the given qualified name in document order.
    ///
    /// The name is marked as used even if there are no such children.
    ///
    /// ```
    /// use use_xml::{NamespaceHandle, parse};
    ///
    /// let doc = use_xml::parse_str("<animals><dog>A</dog><cat>B</cat><dog>D</dog></animals>")?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// let dogs = doc
    ///     .root()
    ///     .children(ns, "dog")?
    ///     .into_iter()
    ///     .map(|dog| dog.text(parse::string))
    ///     .collect::<Result<Vec<_>, _>>()?;
    /// assert_eq!(dogs, ["A", "D"]);
    /// assert!(doc.root().children(ns, "pig")?.is_empty());
    /// // `cat` was not used.
    /// assert!(doc.check_all_used().is_err());
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::AlreadyUsed`] if children with this name were already requested.
    /// - [`ErrorKind::InvalidNamespaceHandle`] if `ns` is out of range of the document
    ///   (a handle of another document is not always detected, see [`NamespaceHandle`]).
    /// - [`ErrorKind::InvalidName`] if `local_name` is not an `NCName`
    ///   or is longer than [`MAX_NAME_LEN`](crate::MAX_NAME_LEN).
    /// - [`ErrorKind::LimitExceeded`] with [`Limit::QualifiedNames`](crate::Limit::QualifiedNames)
    ///   if the name of missing children cannot be interned.
    pub fn children(self, ns: NamespaceHandle, local_name: &str) -> Result<Vec<Elem<'doc>>, Error> {
        self.use_children(ns, local_name, |indices| {
            indices.iter().map(|&i| Elem::new(self.doc, i)).collect()
        })
    }

    /// Returns the child with the given qualified name, or `None` if there is no such child.
    ///
    /// ```
    /// use use_xml::{ErrorKind, NamespaceHandle};
    ///
    /// let doc = use_xml::parse_str("<animals><dog/><dog/></animals>")?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// assert!(doc.root().child_opt(ns, "cat")?.is_none());
    /// assert!(matches!(
    ///     doc.root().child_opt(ns, "dog").unwrap_err().kind(),
    ///     ErrorKind::WrongCount { found: 2, .. }
    /// ));
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::WrongCount`] if there are multiple such children.
    /// - The errors of [`Elem::children`].
    pub fn child_opt(
        self,
        ns: NamespaceHandle,
        local_name: &str,
    ) -> Result<Option<Elem<'doc>>, Error> {
        match self.use_children(ns, local_name, |indices| match indices {
            [] => Ok(None),
            [i] => Ok(Some(*i)),
            _ => Err(indices.len()),
        })? {
            Ok(index) => Ok(index.map(|i| Elem::new(self.doc, i))),
            Err(found) => Err(self.wrong_count(ns, local_name, "at most one", found)),
        }
    }

    /// Returns the only child with the given qualified name.
    ///
    /// ```
    /// use use_xml::{NamespaceHandle, parse};
    ///
    /// let doc = use_xml::parse_str("<animals><pig>B</pig></animals>")?;
    /// let pig = doc.root().child(NamespaceHandle::NO_NAMESPACE, "pig")?;
    /// assert_eq!(pig.text(parse::string)?, "B");
    /// doc.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::Missing`] if there is no such child.
    /// - [`ErrorKind::WrongCount`] if there are multiple such children.
    /// - The errors of [`Elem::children`].
    pub fn child(self, ns: NamespaceHandle, local_name: &str) -> Result<Elem<'doc>, Error> {
        match self.use_children(ns, local_name, |indices| match indices {
            [i] => Ok(*i),
            _ => Err(indices.len()),
        })? {
            Ok(i) => Ok(Elem::new(self.doc, i)),
            Err(0) => Err(ErrorKind::Missing {
                path: self.path(),
                position: self.position(),
                item: Item::Child(self.qualified_name(ns, local_name)),
            }
            .into()),
            Err(found) => Err(self.wrong_count(ns, local_name, "exactly one", found)),
        }
    }

    /// Marks the element and all its descendants as used.
    ///
    /// Any later request on the element or its descendants fails with [`ErrorKind::AlreadyUsed`].
    /// It does not mark the name of the element as used in its parent.
    ///
    /// ```
    /// use use_xml::NamespaceHandle;
    ///
    /// let doc = use_xml::parse_str("<r><header v='1'>x</header><body/></r>")?;
    /// let ns = NamespaceHandle::NO_NAMESPACE;
    /// doc.root().child(ns, "header")?.ignore_all();
    /// doc.root().child(ns, "body")?;
    /// doc.check_all_used()?;
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    pub fn ignore_all(self) {
        let mut stack = vec![self.index];
        while let Some(index) = stack.pop() {
            let node = self.doc.node(index);
            if !node.all_used.replace(true) {
                stack.extend(node.children.borrow().values().flat_map(|(_, elems)| elems));
            }
        }
    }

    /// Checks that everything in the element and its descendants was used.
    ///
    /// Returns [`ErrorKind::Unused`] for the first unused item. For an unused child name
    /// the error contains the path and the position of the first child with this name.
    /// Within an element the order is:
    /// attributes, significant text, child names, and then descendants
    /// of the used child names (in document order within a name).
    /// Attributes and child names are ordered by the order in which their qualified names
    /// first occur in the document.
    /// Text consisting only of whitespace does not need to be used.
    /// Elements marked by [`Elem::ignore_all`] are skipped.
    ///
    /// The traversal does not use recursion.
    ///
    /// ```
    /// use use_xml::{ErrorKind, Item, NamespaceHandle, QualifiedName};
    ///
    /// let doc = use_xml::parse_str(r#"<r><a x="1"/></r>"#)?;
    /// doc.root().child(NamespaceHandle::NO_NAMESPACE, "a")?;
    /// let error = doc.check_all_used().unwrap_err();
    /// assert_eq!(error.path(), Some("/r/a[1]"));
    /// assert_eq!(error.kind(), &ErrorKind::Unused {
    ///     path: "/r/a[1]".to_owned(),
    ///     position: use_xml::Position { line: 1, column: 4 },
    ///     item: Item::Attr(QualifiedName::new("", "x")),
    /// });
    /// # Ok::<(), use_xml::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// - [`ErrorKind::Unused`] for the first unused item.
    pub fn check_all_used(self) -> Result<(), Error> {
        // Children are pushed in reverse order, so they are popped by name and then in document order.
        let mut stack = vec![self.index];
        while let Some(index) = stack.pop() {
            let elem = Elem::new(self.doc, index);
            let node = elem.node();
            if node.all_used.get() {
                continue;
            }
            elem.check_own_items_used()?;
            stack.extend(
                node.children
                    .borrow()
                    .values()
                    .rev()
                    .flat_map(|(_, elems)| elems.iter().rev()),
            );
        }
        Ok(())
    }

    /// Checks the attributes, the text, and the child names of this element, not its descendants.
    fn check_own_items_used(self) -> Result<(), Error> {
        let node = self.node();
        let names = &self.doc.names;
        let unused = |item| {
            ErrorKind::Unused {
                path: self.path(),
                position: self.position(),
                item,
            }
            .into()
        };

        let unused_attr = node
            .attrs
            .borrow()
            .iter()
            .find(|(_, value)| value.is_some())
            .map(|(name, _)| *name);
        if let Some(name) = unused_attr {
            return Err(unused(Item::Attr(names.qualified_name(name))));
        }
        if node.text_significant {
            let text = node.text.take();
            let text_unused = text.is_some();
            node.text.set(text);
            if text_unused {
                return Err(unused(Item::Text));
            }
        }
        let unused_child = node
            .children
            .borrow()
            .iter()
            .find(|(_, (used, _))| !used)
            .map(|(name, (_, elems))| (*name, elems.first().copied()));
        if let Some((name, first)) = unused_child {
            // An unused name always has at least one child (missing names are marked as used).
            let first = first.map_or(self, |i| Elem::new(self.doc, i));
            return Err(ErrorKind::Unused {
                path: first.path(),
                position: first.position(),
                item: Item::Child(names.qualified_name(name)),
            }
            .into());
        }
        Ok(())
    }
}
