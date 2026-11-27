//! Tests of the parsing rules (well-formedness, namespaces, limits),
//! the traversal of deep documents with a small stack, and the messages of all errors.

use use_xml::{
    ErrorKind, Item, Limit, MAX_ATTRS, MAX_CHILD_NAMES, MAX_DEPTH, MAX_NAME_LEN, MAX_NAMESPACE_LEN,
    MAX_NAMESPACES, MAX_PREFIX_LEN, NamespaceHandle, Position, parse, parse_bytes, parse_str,
};

const NO: NamespaceHandle = NamespaceHandle::NO_NAMESPACE;

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
}

fn xml(message: &str, position: Position) -> ErrorKind {
    ErrorKind::Xml {
        message: message.to_owned(),
        position,
    }
}

fn namespace(message: &str, position: Position) -> ErrorKind {
    ErrorKind::Namespace {
        message: message.to_owned(),
        position,
    }
}

fn limit(limit: Limit, position: Position) -> ErrorKind {
    ErrorKind::LimitExceeded {
        limit,
        position: Some(position),
    }
}

fn check(cases: &[(&str, ErrorKind)]) {
    for (input, expected) in cases {
        assert_eq!(parse_str(input).unwrap_err(), *expected, "{input:?}");
    }
}

// ---------------------------------------------------------------------------
// Whitespace.
// ---------------------------------------------------------------------------

#[test]
fn non_xml_whitespace_is_significant() -> anyhow::Result<()> {
    // Only space, tab, carriage return, and line feed are XML whitespace.
    for ws in ["\u{A0}", "\u{2003}", "\u{3000}"] {
        let doc = parse_str(&format!("<r>\n<a>{ws}</a></r>"))?;
        let e = doc.root();
        e.child(NO, "a")?;
        assert_eq!(
            e.check_all_used().unwrap_err(),
            ErrorKind::Unused {
                path: "/r/a[1]".to_owned(),
                position: pos(2, 1),
                item: Item::Text
            },
            "{ws:?}"
        );

        assert_eq!(
            parse_str(&format!("<r>\n<a>{ws}<b/></a></r>")).unwrap_err(),
            ErrorKind::MixedContent {
                path: "/r/a[1]".to_owned(),
                position: pos(2, 1)
            },
            "{ws:?}"
        );

        assert_eq!(
            parse_str(&format!("<a/>\n{ws}")).unwrap_err(),
            ErrorKind::UnexpectedNode {
                node: "text outside of the root element".to_owned(),
                position: pos(2, 1)
            },
            "{ws:?}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Well-formedness.
// ---------------------------------------------------------------------------

#[test]
fn invalid_names_are_rejected() {
    check(&[
        ("<1/>", xml("expected element name", pos(1, 2))),
        ("< a/>", xml("expected element name", pos(1, 2))),
        ("< />", xml("expected element name", pos(1, 2))),
        ("<-a/>", xml("expected element name", pos(1, 2))),
        ("<a 1='x'/>", xml("expected attribute name", pos(1, 4))),
    ]);
    // Name characters which cannot start a name are allowed later.
    assert!(parse_str("<a-b.c\u{B7}9/>").is_ok());
    assert!(parse_str("<_\u{C0}\u{10000}/>").is_ok());
}

#[test]
fn whitespace_is_required_between_attributes() {
    check(&[
        (
            r#"<a x="1"y="2"/>"#,
            xml("expected whitespace, `>`, or `/>`", pos(1, 9)),
        ),
        (
            r#"<a x="1"/ >"#,
            xml("expected whitespace, `>`, or `/>`", pos(1, 9)),
        ),
    ]);
}

#[test]
fn attribute_syntax_errors() {
    check(&[
        (
            "<a x=1/>",
            xml("expected quoted attribute value", pos(1, 6)),
        ),
        (
            "<a\n  x y='1'/>",
            xml("expected `=` after attribute name", pos(2, 5)),
        ),
        (
            "<a x= />",
            xml("expected quoted attribute value", pos(1, 7)),
        ),
        ("<a x='1", xml("attribute value not closed", pos(1, 6))),
        ("<a x='a<b'/>", xml("`<` in attribute value", pos(1, 8))),
        ("<a x='1'", xml("start tag `a` not closed", pos(1, 1))),
    ]);
}

#[test]
fn text_syntax_errors() {
    check(&[
        ("<a>x]]>y</a>", xml("`]]>` in text", pos(1, 5))),
        (
            "<a><![CDATA[x</a>",
            xml("CDATA section not closed", pos(1, 4)),
        ),
        (
            "<a><!x></a>",
            xml("invalid markup in element content", pos(1, 4)),
        ),
        (
            "<a><!DOCTYPE a></a>",
            xml("invalid markup in element content", pos(1, 4)),
        ),
    ]);
    // `]]` and `>` alone are fine.
    assert!(parse_str("<a>]] > ]]</a>").is_ok());
}

#[test]
fn forbidden_characters_are_rejected() {
    check(&[
        ("<a>\u{1}</a>", xml("forbidden character U+0001", pos(1, 4))),
        (
            "<a x='\u{0}'/>",
            xml("forbidden character U+0000", pos(1, 7)),
        ),
        (
            "<!-- \u{1F} --><a/>",
            xml("forbidden character U+001F", pos(1, 6)),
        ),
        (
            "<a>\u{FFFE}</a>",
            xml("forbidden character U+FFFE", pos(1, 4)),
        ),
        (
            "<a>\u{FFFF}</a>",
            xml("forbidden character U+FFFF", pos(1, 4)),
        ),
        (
            "<a>&#1;</a>",
            xml("character reference to a forbidden character", pos(1, 4)),
        ),
        (
            "<a>&#xFFFE;</a>",
            xml("character reference to a forbidden character", pos(1, 4)),
        ),
        (
            "<a>&#xD800;</a>",
            xml("character reference to a forbidden character", pos(1, 4)),
        ),
        (
            "<a>&#x110000;</a>",
            xml("character reference to a forbidden character", pos(1, 4)),
        ),
        (
            "<a>&#99999999999;</a>",
            xml("character reference to a forbidden character", pos(1, 4)),
        ),
        (
            "<a>&#X41;</a>",
            xml("invalid character reference", pos(1, 4)),
        ),
    ]);
    // DEL and other characters are allowed.
    assert!(parse_str("<a>\u{7F}\u{85}\u{FFFD}\u{10FFFF}&#x7F;</a>").is_ok());
}

#[test]
fn xml_declaration_is_checked() {
    check(&[
        (
            "<?xml?><a/>",
            xml("expected `version` in XML declaration", pos(1, 6)),
        ),
        (
            "<?xml encoding='UTF-8'?><a/>",
            xml("expected `version` in XML declaration", pos(1, 7)),
        ),
        (
            "<?xml version='2.0'?><a/>",
            xml("unsupported XML version `2.0`", pos(1, 16)),
        ),
        (
            "<?xml version='1.'?><a/>",
            xml("unsupported XML version `1.`", pos(1, 16)),
        ),
        (
            "<?xml version='1.0' standalone='maybe'?><a/>",
            xml("`standalone` must be `yes` or `no`", pos(1, 33)),
        ),
        (
            "<?xml version='1.0' encoding='8bit'?><a/>",
            xml("invalid encoding name `8bit`", pos(1, 31)),
        ),
        (
            "<?xml version='1.0' standalone='yes' encoding='UTF-8'?><a/>",
            xml("expected `?>` at the end of XML declaration", pos(1, 38)),
        ),
        (
            "<?xml version='1.0'encoding='UTF-8'?><a/>",
            xml("expected `?>` at the end of XML declaration", pos(1, 20)),
        ),
        (
            "<?xml version = \"1.0\" ?",
            xml("expected `?>` at the end of XML declaration", pos(1, 23)),
        ),
    ]);
    assert!(
        parse_str("<?xml version=\"1.1\" encoding=\"latin-2\" standalone=\"no\" ?><a/>").is_ok()
    );
}

#[test]
fn processing_instructions_and_comments_are_checked() {
    check(&[
        (
            "<?XML x?><a/>",
            xml("reserved processing instruction target `XML`", pos(1, 1)),
        ),
        (
            "<a><?xMl?></a>",
            xml("reserved processing instruction target `xMl`", pos(1, 4)),
        ),
        (
            "<??><a/>",
            xml("expected processing instruction target", pos(1, 3)),
        ),
        ("<?pi", xml("processing instruction not closed", pos(1, 1))),
        (
            "<?pi-x?><?pi\u{B7}x?><?a.b?><?pi'x?><a/>",
            xml(
                "expected whitespace after processing instruction target",
                pos(1, 28),
            ),
        ),
        ("<!-- a -- b --><a/>", xml("`--` in comment", pos(1, 8))),
        ("<!-- a ---><a/>", xml("`--` in comment", pos(1, 8))),
        ("<a><!-- x</a>", xml("comment not closed", pos(1, 4))),
        ("<!x><a/>", xml("invalid markup", pos(1, 1))),
    ]);
    assert!(parse_str("<?xml-stylesheet href='a'?><a><?x?><!----></a>").is_ok());
}

#[test]
fn tags_are_checked() {
    check(&[
        (
            "<a></b>",
            xml("end tag `b` does not match start tag `a`", pos(1, 4)),
        ),
        (
            "<p:a xmlns:p='u'></a>",
            xml("end tag `a` does not match start tag `p:a`", pos(1, 18)),
        ),
        (
            "<a></a x>",
            xml("expected `>` at the end of end tag", pos(1, 8)),
        ),
        ("<a>", xml("element `a` not closed", pos(1, 4))),
        ("</a>", xml("end tag without start tag", pos(1, 1))),
        ("<a/></a>", xml("end tag without start tag", pos(1, 5))),
        ("", xml("missing root element", pos(1, 1))),
        ("<!-- -->  ", xml("missing root element", pos(1, 11))),
    ]);
    // Whitespace is allowed before `>` of an end tag and around `=`.
    assert!(parse_str("<a  x = '1' ></a\n>").is_ok());
}

// ---------------------------------------------------------------------------
// Namespaces.
// ---------------------------------------------------------------------------

#[test]
fn qualified_names_are_checked() {
    check(&[
        (
            "<:a/>",
            namespace("element name starts with a colon", pos(1, 2)),
        ),
        (
            "<a:/>",
            namespace("element name has an empty or invalid local name", pos(1, 2)),
        ),
        (
            "<a:1/>",
            namespace("element name has an empty or invalid local name", pos(1, 2)),
        ),
        (
            "<a:b:c/>",
            namespace("element name contains multiple colons", pos(1, 2)),
        ),
        (
            "<a :x='1'/>",
            namespace("attribute name starts with a colon", pos(1, 4)),
        ),
        (
            "<a x:y:z='1'/>",
            namespace("attribute name contains multiple colons", pos(1, 4)),
        ),
        (
            "<?a:b?><a/>",
            namespace("colon in processing instruction target", pos(1, 3)),
        ),
    ]);
}

#[test]
fn prefixes_must_be_declared() {
    check(&[
        ("<p:a/>", namespace("undeclared prefix `p`", pos(1, 2))),
        (
            "<a p:x='1'/>",
            namespace("undeclared prefix `p`", pos(1, 4)),
        ),
        // The declaration is not in scope after the end of its element.
        (
            "<r><a xmlns:p='u'/><p:b/></r>",
            namespace("undeclared prefix `p`", pos(1, 21)),
        ),
        (
            "<xmlns:a/>",
            namespace("prefix `xmlns` must not be used for elements", pos(1, 2)),
        ),
    ]);
}

#[test]
fn declarations_may_follow_their_use_in_a_start_tag() -> anyhow::Result<()> {
    let doc = parse_str("<p:a p:x='1' xmlns:p='urn:p'/>")?;
    let p = doc.namespace("urn:p")?;
    assert_eq!(doc.root().namespace(), p);
    assert_eq!(doc.root().attr(p, "x", parse::u8)?, 1);
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn reserved_prefixes_and_namespaces() -> anyhow::Result<()> {
    let xml_ns = "http://www.w3.org/XML/1998/namespace";
    let xmlns_ns = "http://www.w3.org/2000/xmlns/";
    check(&[
        (
            "<a xmlns:xmlns='urn:x'/>",
            namespace("prefix `xmlns` must not be declared", pos(1, 4)),
        ),
        (
            "<a xmlns:xml='urn:x'/>",
            namespace(
                &format!("prefix `xml` must be bound to `{xml_ns}`"),
                pos(1, 4),
            ),
        ),
        (
            &format!("<a xmlns:x='{xml_ns}'/>"),
            namespace(
                &format!("namespace `{xml_ns}` must be bound only to prefix `xml`"),
                pos(1, 4),
            ),
        ),
        (
            &format!("<a xmlns='{xml_ns}'/>"),
            namespace(
                &format!("namespace `{xml_ns}` must be bound only to prefix `xml`"),
                pos(1, 4),
            ),
        ),
        (
            &format!("<a xmlns:x='{xmlns_ns}'/>"),
            namespace(
                &format!("namespace `{xmlns_ns}` must not be declared"),
                pos(1, 4),
            ),
        ),
        (
            &format!("<a xmlns='{xmlns_ns}'/>"),
            namespace(
                &format!("namespace `{xmlns_ns}` must not be declared"),
                pos(1, 4),
            ),
        ),
        (
            "<a xmlns:p=''/>",
            namespace(
                "prefix `p` must not be bound to the empty namespace",
                pos(1, 4),
            ),
        ),
    ]);
    // Binding `xml` to its own namespace is allowed, `xmlns=""` undeclares the default namespace.
    parse_str(&format!("<a xmlns:xml='{xml_ns}' xmlns=''/>"))?;
    Ok(())
}

#[test]
fn references_in_namespace_declarations_are_decoded() -> anyhow::Result<()> {
    let doc = parse_str("<a xmlns='urn:&amp;&#x41;'/>")?;
    assert_eq!(doc.root().namespace_uri(), "urn:&A");
    check(&[(
        "<a xmlns='&foo;'/>",
        ErrorKind::UnknownEntity {
            name: "foo".to_owned(),
            position: pos(1, 11),
        },
    )]);
    Ok(())
}

#[test]
fn duplicate_attributes() {
    let duplicate = |path: &str, position, name: &str| ErrorKind::DuplicateAttr {
        path: path.to_owned(),
        position,
        name: name.to_owned(),
    };
    check(&[
        (r#"<e x="1" x="2"/>"#, duplicate("/e", pos(1, 10), "x")),
        (
            "<r xmlns:a='urn:x' xmlns:b='urn:x'>\n<e a:x='1' b:x='2'/></r>",
            duplicate("/r/e[1]", pos(2, 12), "b:x"),
        ),
        (
            "<r xmlns:a='urn:a'><e a:x='1' a:x='2'/></r>",
            duplicate("/r/e[1]", pos(1, 31), "a:x"),
        ),
        (
            "<e xmlns='u' xmlns='v'/>",
            duplicate("/e", pos(1, 14), "xmlns"),
        ),
        (
            "<e xmlns:p='u' xmlns:p='u'/>",
            duplicate("/e", pos(1, 16), "xmlns:p"),
        ),
    ]);
}

#[test]
fn same_local_name_in_different_namespaces_is_not_duplicate() -> anyhow::Result<()> {
    let doc = parse_str(
        "<e xmlns:a='urn:a' xmlns:b='urn:b' a:x='1' b:x='2' x='3' xml:lang='cs' lang='en'/>",
    )?;
    let (a, b) = (doc.namespace("urn:a")?, doc.namespace("urn:b")?);
    let e = doc.root();
    assert_eq!(e.attr(a, "x", parse::u8)?, 1);
    assert_eq!(e.attr(b, "x", parse::u8)?, 2);
    assert_eq!(e.attr(NO, "x", parse::u8)?, 3);
    assert_eq!(e.attr(NamespaceHandle::XML, "lang", parse::string)?, "cs");
    assert_eq!(e.attr(NO, "lang", parse::string)?, "en");
    doc.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Limits.
// ---------------------------------------------------------------------------

#[test]
fn name_length_is_limited_in_bytes() -> anyhow::Result<()> {
    let max = "a".repeat(MAX_NAME_LEN);
    let doc = parse_str(&format!("<{max} {max}='1'/>"))?;
    assert_eq!(doc.root().attr(NO, &max, parse::u8)?, 1);
    // `č` has two bytes in UTF-8.
    let long = "č".repeat(MAX_NAME_LEN / 2) + "a";
    check(&[
        (&format!("<{long}/>"), limit(Limit::NameLen, pos(1, 2))),
        (
            &format!("<a {long}='1'/>"),
            limit(Limit::NameLen, pos(1, 4)),
        ),
        (
            &format!("<a xmlns:p='u'><p:{long}/></a>"),
            limit(Limit::NameLen, pos(1, 17)),
        ),
        (&format!("<?{long}?><a/>"), limit(Limit::NameLen, pos(1, 3))),
        (
            &format!("<a>&{long};</a>"),
            limit(Limit::NameLen, pos(1, 5)),
        ),
    ]);
    Ok(())
}

#[test]
fn prefix_length_is_limited() -> anyhow::Result<()> {
    let max = "p".repeat(MAX_PREFIX_LEN);
    let doc = parse_str(&format!("<{max}:a xmlns:{max}='u' {max}:x='1'/>"))?;
    let u = doc.namespace("u")?;
    assert_eq!(doc.root().attr(u, "x", parse::u8)?, 1);
    let long = "p".repeat(MAX_PREFIX_LEN + 1);
    check(&[
        (&format!("<{long}:a/>"), limit(Limit::PrefixLen, pos(1, 2))),
        (
            &format!("<a {long}:x='1'/>"),
            limit(Limit::PrefixLen, pos(1, 4)),
        ),
        (
            &format!("<a xmlns:{long}='u'/>"),
            limit(Limit::PrefixLen, pos(1, 4)),
        ),
    ]);
    Ok(())
}

#[test]
fn namespace_length_is_limited_after_decoding() -> anyhow::Result<()> {
    let max = "u".repeat(MAX_NAMESPACE_LEN);
    let doc = parse_str(&format!("<a xmlns='{max}'/>"))?;
    assert_eq!(doc.root().namespace_uri(), max);
    // `&amp;` is one byte after decoding.
    parse_str(&format!("<a xmlns='&amp;{}'/>", &max[1..]))?;
    check(&[(
        &format!("<a xmlns:p='{max}u'/>"),
        limit(Limit::NamespaceLen, pos(1, 4)),
    )]);
    Ok(())
}

#[test]
fn number_of_namespaces_is_limited() -> anyhow::Result<()> {
    // The empty namespace and the `xml` namespace are always interned.
    let declarations =
        |n: usize| -> String { (0..n).map(|i| format!(" xmlns:p{i}='urn:{i}'")).collect() };
    let doc = parse_str(&format!("<a{}/>", declarations(MAX_NAMESPACES - 2)))?;
    assert_eq!(
        doc.namespace("urn:new").unwrap_err(),
        ErrorKind::LimitExceeded {
            limit: Limit::Namespaces,
            position: None
        }
    );
    // The same namespace declared again does not count.
    parse_str(&format!(
        "<a{}><b xmlns='urn:0'/></a>",
        declarations(MAX_NAMESPACES - 2)
    ))?;
    let input = format!("<a{}/>", declarations(MAX_NAMESPACES - 1));
    let offset = input.rfind(" xmlns").unwrap_or_default() as u32 + 2;
    check(&[(&input, limit(Limit::Namespaces, pos(1, offset)))]);
    Ok(())
}

#[test]
fn number_of_attributes_is_limited() -> anyhow::Result<()> {
    let attrs = |n: usize| -> String { (0..n).map(|i| format!(" a{i}='1'")).collect() };
    let doc = parse_str(&format!("<a{}/>", attrs(MAX_ATTRS)))?;
    assert_eq!(doc.root().attr(NO, "a999", parse::u8)?, 1);
    let input = format!("<a{}/>", attrs(MAX_ATTRS + 1));
    let offset = input.rfind(" a1000").unwrap_or_default() as u32 + 2;
    check(&[(&input, limit(Limit::Attrs, pos(1, offset)))]);
    // Namespace declarations count too.
    let input = format!("<a xmlns='u'{}/>", attrs(MAX_ATTRS));
    let offset = input.rfind(" a999").unwrap_or_default() as u32 + 2;
    check(&[(&input, limit(Limit::Attrs, pos(1, offset)))]);
    Ok(())
}

#[test]
fn number_of_child_names_is_limited() -> anyhow::Result<()> {
    let children = |n: usize| -> String { (0..n).map(|i| format!("<c{i}/>")).collect() };
    parse_str(&format!("<a>{}</a>", children(MAX_CHILD_NAMES)))?;
    let input = format!("<a>{}<c0/></a>", children(MAX_CHILD_NAMES));
    parse_str(&input)?;
    let input = format!("<a>{}</a>", children(MAX_CHILD_NAMES + 1));
    let offset = input.rfind("<c1000").unwrap_or_default() as u32 + 1;
    check(&[(&input, limit(Limit::ChildNames, pos(1, offset)))]);
    // The same child can repeat any number of times.
    let doc = parse_str(&format!("<a>{}</a>", "<b/>".repeat(10_000)))?;
    assert_eq!(doc.root().children(NO, "b")?.len(), 10_000);
    Ok(())
}

#[test]
fn prefix_lookup_does_not_depend_on_number_of_bindings() -> anyhow::Result<()> {
    // Many bindings in scope: `MAX_DEPTH - 2` elements, each with `MAX_ATTRS - 1` declarations.
    // The prefix `p` is bound at the root, so a linear search of the bindings
    // would visit all of them for each prefixed name (several seconds for this document).
    let declarations: String = (0..MAX_ATTRS - 1)
        .map(|i| format!(" xmlns:x{i}='u'"))
        .collect();
    let attrs: String = (0..MAX_ATTRS - 1).map(|i| format!(" p:a{i}='1'")).collect();
    let levels = MAX_DEPTH - 2;
    let input = format!(
        "<r xmlns:p='urn:p'>{}{}{}</r>",
        format!("<e{declarations}>").repeat(levels),
        format!("<p:c{attrs}/>").repeat(20),
        "</e>".repeat(levels)
    );
    let start = std::time::Instant::now();
    let doc = parse_str(&input)?;
    let elapsed = start.elapsed();
    // A generous bound, so the test is reliable also in debug builds on slow machines.
    assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
    // The bindings are restored after each element.
    let mut e = doc.root();
    for _ in 0..levels {
        e = e.child(NO, "e")?;
    }
    let p = doc.namespace("urn:p")?;
    assert_eq!(e.children(p, "c")?.len(), 20);
    Ok(())
}

#[test]
fn bindings_are_restored_after_elements() -> anyhow::Result<()> {
    let doc = parse_str(
        "<a xmlns='urn:1' xmlns:p='urn:p1'>\
            <b xmlns='urn:2' xmlns:p='urn:p2'><c/><p:c/></b>\
            <b xmlns=''><c/><p:c/></b>\
            <b/><p:c/>\
        </a>",
    )?;
    let ns = |uri: &str| doc.namespace(uri);
    let (n1, n2, p1, p2) = (ns("urn:1")?, ns("urn:2")?, ns("urn:p1")?, ns("urn:p2")?);
    let a = doc.root();
    assert_eq!(a.namespace(), n1);
    a.child(n1, "b")?;
    let b2 = a.child(n2, "b")?;
    b2.child(n2, "c")?;
    b2.child(p2, "c")?;
    let b0 = a.child(NO, "b")?;
    b0.child(NO, "c")?;
    b0.child(p1, "c")?;
    a.child(p1, "c")?;
    doc.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Deep documents with a small stack.
// ---------------------------------------------------------------------------

/// Stack size of the threads below. Parsing, `check_all_used`, `ignore_all`, paths,
/// and dropping do not use recursion.
const SMALL_STACK: usize = 64 * 1024;

/// A document nested `MAX_DEPTH` levels deep with an attribute on the deepest element.
fn deepest_document() -> String {
    format!(
        "{}<a x='1'/>{}",
        "<a>".repeat(MAX_DEPTH - 1),
        "</a>".repeat(MAX_DEPTH - 1)
    )
}

fn run_with_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("thread can be spawned")
        .join()
        .expect("thread does not panic");
}

#[test]
fn deep_document_needs_little_stack() {
    run_with_small_stack(|| {
        let doc = parse_str(&deepest_document()).expect("document is valid");
        let mut elem = doc.root();
        while let Some(child) = elem.child_opt(NO, "a").expect("not used yet") {
            elem = child;
        }
        assert_eq!(
            doc.check_all_used().unwrap_err(),
            ErrorKind::Unused {
                path: "/a".to_owned() + &"/a[1]".repeat(MAX_DEPTH - 1),
                position: pos(1, 3 * MAX_DEPTH as u32 - 2),
                item: Item::attr("", "x")
            }
        );
        doc.root().ignore_all();
        doc.check_all_used().expect("everything is ignored");
        drop(doc);
    });
}

#[test]
fn unfinished_deep_document_needs_little_stack() {
    let input = "<a>".repeat(MAX_DEPTH);
    run_with_small_stack(move || {
        assert_eq!(
            parse_str(&input).unwrap_err(),
            xml("element `a` not closed", pos(1, 3 * MAX_DEPTH as u32 + 1))
        );
    });
}

#[test]
fn check_all_used_goes_deep_before_next_name() -> anyhow::Result<()> {
    let doc = parse_str("<r><b y='2'/><a><c z='3'/></a><a w='4'/></r>")?;
    let e = doc.root();
    e.children(NO, "a")?[0].child(NO, "c")?;
    e.child(NO, "b")?;
    // `b` before `a` (by the first occurrence), the first `a` with its descendants
    // before the second `a`.
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/b[1]".to_owned(),
            position: pos(1, 4),
            item: Item::attr("", "y")
        }
    );
    Ok(())
}

#[test]
fn check_all_used_orders_names_by_first_occurrence() -> anyhow::Result<()> {
    let doc = parse_str("<r><a z='1' y='2'/><y/></r>")?;
    let e = doc.root();
    let a = e.child(NO, "a")?;
    // `z` occurs before `y` (the attribute and the element `y` have the same qualified name).
    assert_eq!(
        doc.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/y[1]".to_owned(),
            position: pos(1, 20),
            item: Item::child("", "y")
        }
    );
    e.child(NO, "y")?;
    assert_eq!(
        doc.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/a[1]".to_owned(),
            position: pos(1, 4),
            item: Item::attr("", "z")
        }
    );
    a.attr(NO, "z", parse::u8)?;
    a.attr(NO, "y", parse::u8)?;
    doc.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Messages of all errors.
// ---------------------------------------------------------------------------

#[test]
fn error_messages() -> anyhow::Result<()> {
    let message = |input: &str| parse_str(input).unwrap_err().to_string();

    assert_eq!(
        message("<a>\n<b></c></a>"),
        "malformed XML at line 2, column 4: end tag `c` does not match start tag `b`"
    );
    assert_eq!(
        message("<p:a/>"),
        "namespace error at line 1, column 2: undeclared prefix `p`"
    );
    assert_eq!(
        parse_bytes(b"<?xml version='1.0' encoding='latin1'?><a/>")
            .unwrap_err()
            .to_string(),
        "unsupported encoding at line 1, column 31: encoding `latin1` is not supported"
    );
    assert_eq!(
        message("<!DOCTYPE a><a/>"),
        "unexpected DOCTYPE at line 1, column 1"
    );
    assert_eq!(
        message("<a xmlns:p='u' p:x='1' p:x='2'/>"),
        "element /a contains duplicate attribute `p:x` at line 1, column 24"
    );
    assert_eq!(
        message(&format!("<{}/>", "a".repeat(MAX_NAME_LEN + 1))),
        "limit exceeded at line 1, column 2: name longer than 512 bytes"
    );
    assert_eq!(
        ErrorKind::LimitExceeded {
            limit: Limit::Namespaces,
            position: None
        }
        .to_string(),
        "limit exceeded: more than 256 namespaces"
    );
    assert_eq!(
        message("<a>\n&foo;</a>"),
        "unknown entity `foo` at line 2, column 1"
    );
    assert_eq!(
        message("<a><b/>x</a>"),
        "element /a at line 1, column 1 contains mixed content"
    );
    assert_eq!(
        message(&"<a>".repeat(MAX_DEPTH + 1)),
        format!(
            "element /a{} contains a child at line 1, column {} which exceeds the maximum depth 256",
            "/a[1]".repeat(MAX_DEPTH - 1),
            3 * MAX_DEPTH + 1
        )
    );

    let doc = parse_str("<r x='a' xmlns:p='urn:p'><b/><b/></r>")?;
    let e = doc.root();
    let p = doc.namespace("urn:p")?;
    assert_eq!(
        e.attr(NO, "y", parse::string).unwrap_err().to_string(),
        "element /r at line 1, column 1 has no attribute `y`"
    );
    assert_eq!(
        e.attr(p, "y", parse::string).unwrap_err().to_string(),
        "element /r at line 1, column 1 has no attribute `{urn:p}y`"
    );
    assert_eq!(
        e.child(NO, "c").unwrap_err().to_string(),
        "element /r at line 1, column 1 has no child `c`"
    );
    assert_eq!(
        e.child(NO, "b").unwrap_err().to_string(),
        "element /r at line 1, column 1 has 2 children `b`, expected exactly one"
    );
    assert_eq!(
        e.child_opt(NO, "b").unwrap_err().to_string(),
        "child `b` of element /r at line 1, column 1 already used"
    );
    assert_eq!(
        e.attr(NO, "x", parse::u8).unwrap_err().to_string(),
        r#"invalid attribute `x` of element /r at line 1, column 1: expected u8, got "a""#
    );
    // A handle of another document is detected only if it is out of range of this document.
    let other_doc = parse_str("<a/>")?;
    other_doc.namespace("urn:1")?;
    let other = other_doc.namespace("urn:2")?;
    assert_eq!(
        e.child(other, "c").unwrap_err().to_string(),
        "element /r at line 1, column 1 received a namespace handle of another document"
    );
    // `child` marked `b` as used even though it failed.
    e.check_all_used()?;

    let doc = parse_str("<r><b/><b/></r>")?;
    let e = doc.root();
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "element /r/b[1] at line 1, column 4 not used"
    );
    assert_eq!(
        e.child_opt(NO, "b").unwrap_err().to_string(),
        "element /r at line 1, column 1 has 2 children `b`, expected at most one"
    );

    let doc = parse_str("<r x='1'>t</r>")?;
    let e = doc.root();
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "attribute `x` of element /r at line 1, column 1 not used"
    );
    e.attr(NO, "x", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "text of element /r at line 1, column 1 not used"
    );
    assert_eq!(
        e.text(parse::bool).unwrap_err().to_string(),
        r#"invalid text of element /r at line 1, column 1: expected bool, got "t""#
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// No panics.
// ---------------------------------------------------------------------------

#[test]
fn prefixes_and_mutations_of_documents_do_not_panic() {
    let input = "\u{FEFF}<?xml version='1.0' encoding='UTF-8'?><!-- c --><?pi x?>\r\n\
        <p:r xmlns:p='urn:p' xmlns='urn:d' a='&amp;&#x41;\t' p:b=\"č\">\
        <x><![CDATA[ ]]></x><y/>text&lt;<z:q xmlns:z='urn:z'/></p:r>";
    for (i, _) in input.char_indices() {
        let _ = parse_str(&input[..i]);
        let _ = parse_bytes(&input.as_bytes()[..i]);
    }
    let bytes = input.as_bytes();
    for i in 0..bytes.len() {
        for replacement in [
            b'<', b'>', b'&', b':', b'\'', b'"', b'/', b'?', b'!', b' ', 0xFF,
        ] {
            let mut mutated = bytes.to_vec();
            mutated[i] = replacement;
            let _ = parse_bytes(&mutated);
        }
    }
}
