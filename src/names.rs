//! Interning of namespaces and qualified names.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;

/// Maximum length of a local name of an element or an attribute in bytes
/// (also of a processing instruction target and of an entity name).
pub const MAX_NAME_LEN: usize = 512;

/// Maximum length of a namespace name (URI) in bytes.
pub const MAX_NAMESPACE_LEN: usize = 4096;

/// Maximum length of a namespace prefix in bytes.
pub const MAX_PREFIX_LEN: usize = 64;

/// Maximum number of different namespaces in one [`Document`](crate::Document),
/// including the empty namespace and the `xml` namespace, which are always interned.
pub const MAX_NAMESPACES: usize = 256;

/// Maximum number of attributes of one element, including namespace declarations.
pub const MAX_ATTRS: usize = 1000;

/// Maximum number of different qualified names of children of one element.
/// A child with the same qualified name can repeat any number of times.
pub const MAX_CHILD_NAMES: usize = 1000;

/// The namespace bound to the prefix `xml`.
pub(crate) const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// The namespace of namespace declarations. It must not be declared.
pub(crate) const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// An interned namespace name (URI) of one [`Document`](crate::Document).
///
/// Get it by [`Document::namespace`](crate::Document::namespace).
/// A handle is valid only for the document which created it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceHandle(pub(crate) u8);

impl NamespaceHandle {
    /// The empty namespace (no namespace). The same as `doc.namespace("")`.
    ///
    /// Unprefixed attributes and unprefixed elements without a default namespace
    /// declaration are in this namespace.
    pub const NO_NAMESPACE: NamespaceHandle = NamespaceHandle(0);

    /// The namespace bound to the prefix `xml` (`http://www.w3.org/XML/1998/namespace`).
    pub const XML: NamespaceHandle = NamespaceHandle(1);

    fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// An interned qualified name: a namespace and a local name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct QualifiedNameHandle(pub(crate) u32);

impl QualifiedNameHandle {
    fn index(self) -> usize {
        // `u32` always fits into `usize` on the supported (32-bit and 64-bit) targets.
        self.0 as usize
    }
}

/// A qualified name in an error: a namespace name and a local name.
///
/// It is displayed as the local name if the namespace is empty,
/// and in the Clark notation `{namespace}local` otherwise.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedName {
    /// The namespace name (URI); empty for no namespace.
    pub namespace: String,
    /// The local name.
    pub local_name: String,
}

impl QualifiedName {
    /// Creates a qualified name.
    ///
    /// ```
    /// let name = use_xml::QualifiedName::new("urn:x", "item");
    /// assert_eq!(name.to_string(), "{urn:x}item");
    /// assert_eq!(use_xml::QualifiedName::new("", "item").to_string(), "item");
    /// ```
    pub fn new(namespace: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            local_name: local_name.into(),
        }
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_empty() {
            f.write_str(&self.local_name)
        } else {
            write!(f, "{{{}}}{}", self.namespace, self.local_name)
        }
    }
}

/// Why interning failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InternError {
    TooManyNamespaces,
    TooManyQualifiedNames,
}

/// One level of interned names. Handles of a level start at the given offsets,
/// so the handles of the level created after parsing continue after the handles of the parser.
#[derive(Default)]
struct Names {
    namespace_offset: usize,
    name_offset: usize,
    namespaces: Vec<Box<str>>,
    namespace_map: BTreeMap<Box<str>, NamespaceHandle>,
    names: Vec<(NamespaceHandle, Box<str>)>,
    // Indexed by the namespace, then by the local name.
    name_map: BTreeMap<NamespaceHandle, BTreeMap<Box<str>, QualifiedNameHandle>>,
}

impl Names {
    fn find_namespace(&self, uri: &str) -> Option<NamespaceHandle> {
        self.namespace_map.get(uri).copied()
    }

    fn add_namespace(&mut self, uri: &str) -> Result<NamespaceHandle, InternError> {
        let index = self.namespace_offset + self.namespaces.len();
        if index >= MAX_NAMESPACES {
            return Err(InternError::TooManyNamespaces);
        }
        let handle =
            NamespaceHandle(u8::try_from(index).map_err(|_| InternError::TooManyNamespaces)?);
        self.namespaces.push(uri.into());
        self.namespace_map.insert(uri.into(), handle);
        Ok(handle)
    }

    fn namespace_uri(&self, ns: NamespaceHandle) -> Option<&str> {
        let i = ns.index().checked_sub(self.namespace_offset)?;
        self.namespaces.get(i).map(|s| &**s)
    }

    fn find_name(&self, ns: NamespaceHandle, local: &str) -> Option<QualifiedNameHandle> {
        self.name_map.get(&ns)?.get(local).copied()
    }

    fn add_name(
        &mut self,
        ns: NamespaceHandle,
        local: &str,
    ) -> Result<QualifiedNameHandle, InternError> {
        let index = self.name_offset + self.names.len();
        let handle = QualifiedNameHandle(
            u32::try_from(index).map_err(|_| InternError::TooManyQualifiedNames)?,
        );
        self.names.push((ns, local.into()));
        self.name_map
            .entry(ns)
            .or_default()
            .insert(local.into(), handle);
        Ok(handle)
    }

    fn name(&self, name: QualifiedNameHandle) -> Option<(NamespaceHandle, &str)> {
        let i = name.index().checked_sub(self.name_offset)?;
        self.names.get(i).map(|(ns, local)| (*ns, &**local))
    }
}

/// Interner used by the parser. When parsing ends it becomes the immutable part of [`NameTable`].
pub(crate) struct Interner {
    names: Names,
    // Prefixed names as written in the document, used in paths.
    raw_names: Vec<Box<str>>,
    raw_name_map: BTreeMap<Box<str>, u32>,
}

impl Interner {
    pub(crate) fn new() -> Self {
        let mut names = Names::default();
        // The first two namespaces always fit.
        let _ = names.add_namespace("");
        let _ = names.add_namespace(XML_NAMESPACE);
        Self {
            names,
            raw_names: Vec::new(),
            raw_name_map: BTreeMap::new(),
        }
    }

    pub(crate) fn namespace(&mut self, uri: &str) -> Result<NamespaceHandle, InternError> {
        match self.names.find_namespace(uri) {
            Some(handle) => Ok(handle),
            None => self.names.add_namespace(uri),
        }
    }

    pub(crate) fn name(
        &mut self,
        ns: NamespaceHandle,
        local: &str,
    ) -> Result<QualifiedNameHandle, InternError> {
        match self.names.find_name(ns, local) {
            Some(handle) => Ok(handle),
            None => self.names.add_name(ns, local),
        }
    }

    pub(crate) fn raw_name(&mut self, raw: &str) -> Result<u32, InternError> {
        if let Some(&handle) = self.raw_name_map.get(raw) {
            return Ok(handle);
        }
        let handle =
            u32::try_from(self.raw_names.len()).map_err(|_| InternError::TooManyQualifiedNames)?;
        self.raw_names.push(raw.into());
        self.raw_name_map.insert(raw.into(), handle);
        Ok(handle)
    }

    pub(crate) fn get_raw_name(&self, handle: u32) -> &str {
        self.raw_names.get(handle as usize).map_or("", |s| s)
    }

    pub(crate) fn finish(self) -> NameTable {
        let extra = Names {
            namespace_offset: self.names.namespaces.len(),
            name_offset: self.names.names.len(),
            ..Names::default()
        };
        NameTable {
            frozen: self.names,
            raw_names: self.raw_names,
            extra: RefCell::new(extra),
        }
    }
}

/// Names of a parsed document.
///
/// Names from the document are immutable, so references to them can be returned.
/// Names requested later (namespaces by [`Document::namespace`](crate::Document::namespace),
/// qualified names of missing attributes and children) are added to `extra`.
pub(crate) struct NameTable {
    frozen: Names,
    raw_names: Vec<Box<str>>,
    // A borrow never outlives a method of this type, so it never conflicts.
    extra: RefCell<Names>,
}

impl NameTable {
    pub(crate) fn namespace(&self, uri: &str) -> Result<NamespaceHandle, InternError> {
        if let Some(handle) = self.frozen.find_namespace(uri) {
            return Ok(handle);
        }
        let mut extra = self.extra.borrow_mut();
        match extra.find_namespace(uri) {
            Some(handle) => Ok(handle),
            None => extra.add_namespace(uri),
        }
    }

    pub(crate) fn is_valid_namespace(&self, ns: NamespaceHandle) -> bool {
        let extra = self.extra.borrow();
        ns.index() < extra.namespace_offset + extra.namespaces.len()
    }

    /// Namespace name of a handle from the document.
    pub(crate) fn frozen_namespace_uri(&self, ns: NamespaceHandle) -> &str {
        self.frozen.namespace_uri(ns).unwrap_or("")
    }

    /// Namespace name of any handle. Names from the document are borrowed,
    /// names interned later are copied because they are behind a `RefCell`.
    pub(crate) fn namespace_uri_cow(&self, ns: NamespaceHandle) -> Cow<'_, str> {
        match self.frozen.namespace_uri(ns) {
            Some(uri) => Cow::Borrowed(uri),
            None => Cow::Owned(
                self.extra
                    .borrow()
                    .namespace_uri(ns)
                    .unwrap_or_default()
                    .to_owned(),
            ),
        }
    }

    pub(crate) fn namespace_uri(&self, ns: NamespaceHandle) -> String {
        self.namespace_uri_cow(ns).into_owned()
    }

    pub(crate) fn find_name(
        &self,
        ns: NamespaceHandle,
        local: &str,
    ) -> Option<QualifiedNameHandle> {
        self.frozen
            .find_name(ns, local)
            .or_else(|| self.extra.borrow().find_name(ns, local))
    }

    pub(crate) fn intern_name(
        &self,
        ns: NamespaceHandle,
        local: &str,
    ) -> Result<QualifiedNameHandle, InternError> {
        match self.find_name(ns, local) {
            Some(handle) => Ok(handle),
            None => self.extra.borrow_mut().add_name(ns, local),
        }
    }

    /// Namespace and local name of a handle from the document.
    pub(crate) fn frozen_name(&self, name: QualifiedNameHandle) -> (NamespaceHandle, &str) {
        self.frozen
            .name(name)
            .unwrap_or((NamespaceHandle::NO_NAMESPACE, ""))
    }

    pub(crate) fn qualified_name(&self, name: QualifiedNameHandle) -> QualifiedName {
        let (ns, local) = match self.frozen.name(name) {
            Some((ns, local)) => (ns, local.to_owned()),
            None => {
                let extra = self.extra.borrow();
                let (ns, local) = extra
                    .name(name)
                    .unwrap_or((NamespaceHandle::NO_NAMESPACE, ""));
                (ns, local.to_owned())
            }
        };
        QualifiedName {
            namespace: self.namespace_uri(ns),
            local_name: local,
        }
    }

    pub(crate) fn raw_name(&self, handle: u32) -> &str {
        self.raw_names.get(handle as usize).map_or("", |s| s)
    }
}
