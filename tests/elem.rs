use use_xml::{
    Error, ErrorKind, Item, MAX_DEPTH, MAX_VALUE_CHARS, NamespaceHandle, Position, QualifiedName,
    parse, parse_str,
};

const NO: NamespaceHandle = NamespaceHandle::NO_NAMESPACE;

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
}

fn attr_item(namespace: &str, local_name: &str) -> Item {
    Item::Attr(QualifiedName::new(namespace, local_name))
}

fn child_item(namespace: &str, local_name: &str) -> Item {
    Item::Child(QualifiedName::new(namespace, local_name))
}

// ---------------------------------------------------------------------------
// Basic usage (ported from UseData.Xml).
// ---------------------------------------------------------------------------

#[test]
fn xml_declaration() -> anyhow::Result<()> {
    let doc = parse_str(r#"<?xml version="1.0" encoding="utf-8"?><animals />"#)?;
    let e = doc.root();
    assert_eq!(e.local_name(), "animals");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn child_elements() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"
    <animals>
        <dog>A</dog>
        <pig>B</pig>
        <dog>D</dog>
    </animals>
    "#,
    )?;
    let e = doc.root();
    let dogs = e
        .children(NO, "dog")?
        .iter()
        .map(|d| d.text(parse::string))
        .collect::<Result<Vec<_>, _>>()?;
    let pig = e.child(NO, "pig")?.text(parse::string)?;
    let cat = e.child_opt(NO, "cat")?;
    assert_eq!(dogs, ["A", "D"]);
    assert_eq!(pig, "B");
    assert!(cat.is_none());
    e.check_all_used()?;
    Ok(())
}

#[test]
fn attributes() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"
    <people>
        <person name="Pete" />
        <person name="Harry" address="Little Whinging" />
    </people>
    "#,
    )?;
    let e = doc.root();
    let people = e
        .children(NO, "person")?
        .iter()
        .map(|p| {
            Ok((
                p.attr(NO, "name", parse::string)?,
                p.attr_opt(NO, "address", parse::string)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    assert_eq!(
        people,
        [
            ("Pete".to_owned(), None),
            ("Harry".to_owned(), Some("Little Whinging".to_owned()))
        ]
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn prefixes() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"
    <root>
        <h:table xmlns:h="http://www.w3.org/TR/html4/">
            <h:tr>
                <h:td>Cell</h:td>
            </h:tr>
        </h:table>
    </root>
    "#,
    )?;
    let e = doc.root();
    let html = doc.namespace("http://www.w3.org/TR/html4/")?;
    // The child is not in the empty namespace.
    assert_eq!(
        e.child(NO, "table").unwrap_err(),
        ErrorKind::Missing {
            path: "/root".to_owned(),
            position: pos(2, 5),
            item: child_item("", "table")
        }
    );
    let cell = e
        .child(html, "table")?
        .child(html, "tr")?
        .child(html, "td")?;
    assert_eq!(cell.text(parse::string)?, "Cell");
    assert_eq!(cell.prefixed_name(), "h:td");
    assert_eq!(cell.local_name(), "td");
    assert_eq!(cell.namespace(), html);
    assert_eq!(cell.namespace_uri(), "http://www.w3.org/TR/html4/");
    // `xmlns:h` is not an attribute.
    e.check_all_used()?;
    Ok(())
}

#[test]
fn default_namespace() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"
    <root>
        <table xmlns="http://www.w3.org/TR/html4/">
            <tr>
                <td>Cell</td>
            </tr>
        </table>
    </root>
    "#,
    )?;
    let e = doc.root();
    let html = doc.namespace("http://www.w3.org/TR/html4/")?;
    let cell = e
        .child(html, "table")?
        .child(html, "tr")?
        .child(html, "td")?;
    assert_eq!(cell.text(parse::string)?, "Cell");
    assert_eq!(cell.path(), "/root/table[1]/tr[1]/td[1]");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn example_from_specification() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"
<x xmlns:edi='http://ecommerce.example.org/schema'>
  <lineItem edi:taxClass="exempt">Baby food</lineItem>
</x>"#,
    )?;
    let root = doc.root();
    let default_ns = doc.namespace("")?;
    let edi_ns = doc.namespace("http://ecommerce.example.org/schema")?;
    let item = root.child(default_ns, "lineItem")?;
    let tax_class = item.attr(edi_ns, "taxClass", parse::string)?;
    assert_eq!(tax_class, "exempt");
    // The attribute is not in the default namespace.
    assert_eq!(item.attr_opt(default_ns, "taxClass", parse::string)?, None);
    assert_eq!(item.text(parse::string)?, "Baby food");
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn prefixed_attributes_are_in_their_namespace() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a xmlns:p="urn:p" p:x="1" x="2" />"#)?;
    let e = doc.root();
    let p = doc.namespace("urn:p")?;
    assert_eq!(e.attr(p, "x", parse::i32)?, 1);
    assert_eq!(e.attr(NO, "x", parse::i32)?, 2);
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn unprefixed_attributes_are_not_in_default_namespace() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a xmlns="urn:d" x="1" />"#)?;
    let d = doc.namespace("urn:d")?;
    assert_eq!(doc.root().namespace(), d);
    assert_eq!(doc.root().attr_opt(d, "x", parse::i32)?, None);
    assert_eq!(doc.root().attr(NO, "x", parse::i32)?, 1);
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn different_prefixes_with_same_namespace_are_the_same_name() -> anyhow::Result<()> {
    let doc =
        parse_str(r#"<r xmlns:a="urn:x" xmlns:b="urn:x" xmlns="urn:x"><a:i/><b:i/><i/></r>"#)?;
    let x = doc.namespace("urn:x")?;
    let items = doc.root().children(x, "i")?;
    let paths: Vec<String> = items.iter().map(|i| i.path()).collect();
    assert_eq!(paths, ["/r/a:i[1]", "/r/b:i[2]", "/r/i[3]"]);
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn same_local_name_in_different_namespaces() -> anyhow::Result<()> {
    let doc = parse_str(r#"<r xmlns:a="urn:a"><a:x/><a:x/><x/></r>"#)?;
    let a = doc.namespace("urn:a")?;
    let e = doc.root();
    assert_eq!(e.children(a, "x")?.len(), 2);
    let x = e.child(NO, "x")?;
    assert_eq!(x.path(), "/r/x[1]");
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn namespace_declarations_are_scoped() -> anyhow::Result<()> {
    let doc =
        parse_str(r#"<r xmlns="urn:1"><a xmlns="urn:2"><b/></a><c/><d xmlns=""><e/></d></r>"#)?;
    let n1 = doc.namespace("urn:1")?;
    let n2 = doc.namespace("urn:2")?;
    let r = doc.root();
    assert_eq!(r.child(n2, "a")?.child(n2, "b")?.namespace(), n2);
    assert_eq!(r.child(n1, "c")?.namespace(), n1);
    // `xmlns=""` applies to the element itself too.
    assert_eq!(r.child(NO, "d")?.child(NO, "e")?.namespace(), NO);
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn xml_prefix_is_predeclared() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a xml:lang="cs" lang="en"/>"#)?;
    assert_eq!(
        doc.namespace("http://www.w3.org/XML/1998/namespace")?,
        NamespaceHandle::XML
    );
    assert_eq!(
        doc.root()
            .attr(NamespaceHandle::XML, "lang", parse::string)?,
        "cs"
    );
    assert_eq!(doc.root().attr(NO, "lang", parse::string)?, "en");
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn namespaces_not_in_document_are_interned() -> anyhow::Result<()> {
    let doc = parse_str("<a/>")?;
    assert_eq!(doc.namespace("")?, NO);
    let other = doc.namespace("urn:other")?;
    assert_eq!(doc.namespace("urn:other")?, other);
    assert_eq!(doc.root().attr_opt(other, "x", parse::string)?, None);
    assert_eq!(
        doc.root().child(other, "y").unwrap_err(),
        ErrorKind::Missing {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: child_item("urn:other", "y")
        }
    );
    assert_eq!(
        doc.root().children(other, "y").unwrap_err(),
        already_used("/a", pos(1, 1), child_item("urn:other", "y"))
    );
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn namespace_handle_of_another_document_is_rejected() -> anyhow::Result<()> {
    let doc = parse_str("<a/>")?;
    let other = parse_str("<a/>")?;
    let ns = other.namespace("urn:other")?;
    let invalid = ErrorKind::InvalidNamespaceHandle {
        path: "/a".to_owned(),
        position: pos(1, 1),
    };
    let e = doc.root();
    assert_eq!(e.attr_opt(ns, "x", parse::string).unwrap_err(), invalid);
    assert_eq!(e.attr(ns, "x", parse::string).unwrap_err(), invalid);
    assert_eq!(e.child(ns, "x").unwrap_err(), invalid);
    assert_eq!(e.child_opt(ns, "x").unwrap_err(), invalid);
    assert_eq!(e.children(ns, "x").unwrap_err(), invalid);
    Ok(())
}

#[test]
fn too_many_namespaces() -> anyhow::Result<()> {
    let doc = parse_str("<a/>")?;
    // The empty namespace and the `xml` namespace are always interned.
    for i in 2..use_xml::MAX_NAMESPACES {
        doc.namespace(&format!("urn:{i}"))?;
    }
    assert_eq!(
        doc.namespace("urn:last").unwrap_err(),
        ErrorKind::LimitExceeded {
            limit: use_xml::Limit::Namespaces,
            position: None
        }
    );
    // Existing namespaces are still found.
    assert_eq!(doc.namespace("urn:2")?, doc.namespace("urn:2")?);
    Ok(())
}

#[test]
fn parent_and_document() -> anyhow::Result<()> {
    let doc = parse_str("<a><b/></a>")?;
    let b = doc.root().child(NO, "b")?;
    assert_eq!(b.parent().map(|p| p.path()), Some("/a".to_owned()));
    assert!(doc.root().parent().is_none());
    assert!(std::ptr::eq(b.document(), &doc));
    Ok(())
}

// ---------------------------------------------------------------------------
// Text content.
// ---------------------------------------------------------------------------

#[test]
fn whitespace_text_is_kept_without_children() -> anyhow::Result<()> {
    let doc = parse_str("<a>  \n </a>")?;
    let e = doc.root();
    // Whitespace-only text does not need to be used.
    e.check_all_used()?;
    assert_eq!(e.text(parse::string)?, "  \n ");
    Ok(())
}

#[test]
fn text_of_element_with_children_is_empty() -> anyhow::Result<()> {
    let doc = parse_str("<a>\n  <b/>\n  <c/>\n</a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "");
    Ok(())
}

#[test]
fn text_of_empty_element_is_empty() -> anyhow::Result<()> {
    let doc = parse_str("<r><a/><b></b></r>")?;
    let e = doc.root();
    assert_eq!(e.child(NO, "a")?.text(parse::string)?, "");
    assert_eq!(e.child(NO, "b")?.text(parse::string)?, "");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn cdata_and_text_are_concatenated() -> anyhow::Result<()> {
    let doc = parse_str("<a> x <![CDATA[<y>]]> z </a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, " x <y> z ");
    Ok(())
}

#[test]
fn empty_cdata_is_significant() -> anyhow::Result<()> {
    let doc = parse_str("<a><![CDATA[]]></a>")?;
    let e = doc.root();
    assert_eq!(
        e.check_all_used().map_err(Error::into_kind),
        Err(ErrorKind::Unused {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Text
        })
    );
    Ok(())
}

#[test]
fn entities_are_decoded() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a x="&lt;&#65;&quot;">&amp;&#x42;&apos;&gt;</a>"#)?;
    let e = doc.root();
    assert_eq!(e.attr(NO, "x", parse::string)?, "<A\"");
    assert_eq!(e.text(parse::string)?, "&B'>");
    Ok(())
}

#[test]
fn whitespace_char_reference_is_not_significant() -> anyhow::Result<()> {
    let doc = parse_str("<a>&#32;<b/></a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "");
    Ok(())
}

#[test]
fn comments_and_processing_instructions_are_ignored() -> anyhow::Result<()> {
    let doc = parse_str("<!-- c --><?pi x?><a>x<!-- c -->y<?pi?></a><!-- c -->")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "xy");
    Ok(())
}

#[test]
fn line_endings_are_normalized() -> anyhow::Result<()> {
    let doc = parse_str("<a>x\r\ny\rz</a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "x\ny\nz");
    Ok(())
}

// ---------------------------------------------------------------------------
// Structural errors.
// ---------------------------------------------------------------------------

#[test]
fn mixed_content_text_before_child() {
    let input = "<country>\n  Czech Republic\n  <president>Vaclav Havel</president>\n</country>";
    assert_eq!(
        parse_str(input).unwrap_err(),
        ErrorKind::MixedContent {
            path: "/country".to_owned(),
            position: pos(2, 3)
        }
    );
}

#[test]
fn mixed_content_points_to_the_first_significant_text() {
    // Whitespace, comments, and processing instructions before the text are skipped,
    // the first significant text is reported even if more text follows.
    let mixed = |position| ErrorKind::MixedContent {
        path: "/a".to_owned(),
        position,
    };
    let cases = [
        (
            "<a>\r\n \t<!-- c --><?pi?> čx<!---->y<b/></a>",
            mixed(pos(2, 20)),
        ),
        ("<a> &#32; <![CDATA[x]]>y<b/></a>", mixed(pos(1, 11))),
        ("<a><b/>\n <!-- c -->\n  &lt;</a>", mixed(pos(3, 3))),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_str(input).unwrap_err(), expected, "{input:?}");
    }
}

#[test]
fn mixed_content_text_after_child() {
    assert_eq!(
        parse_str("<r><a><b/>text</a></r>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/r/a[1]".to_owned(),
            position: pos(1, 11)
        }
    );
}

#[test]
fn mixed_content_cdata_after_child() {
    assert_eq!(
        parse_str("<r>\n<a><b/><![CDATA[]]></a></r>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/r/a[1]".to_owned(),
            position: pos(2, 8)
        }
    );
}

#[test]
fn mixed_content_entity_reference_after_child() {
    assert_eq!(
        parse_str("<a><b/>&amp;</a>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/a".to_owned(),
            position: pos(1, 8)
        }
    );
}

#[test]
fn mixed_content_entity_reference_before_child() {
    assert_eq!(
        parse_str("<a>&#65;<b/></a>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/a".to_owned(),
            position: pos(1, 4)
        }
    );
}

#[test]
fn unknown_entity() {
    assert_eq!(
        parse_str("<a>x &nbsp;</a>").unwrap_err(),
        ErrorKind::UnknownEntity {
            name: "nbsp".to_owned(),
            position: pos(1, 6)
        }
    );
}

#[test]
fn doctype_is_not_allowed() {
    assert_eq!(
        parse_str("<?xml version=\"1.0\"?>\n<!DOCTYPE a>\n<a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "DOCTYPE".to_owned(),
            position: pos(2, 1)
        }
    );
}

#[test]
fn text_before_root_is_not_allowed() {
    assert_eq!(
        parse_str("x<a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "text outside of the root element".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn text_after_root_is_not_allowed() {
    assert_eq!(
        parse_str("<a/>\n x").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "text outside of the root element".to_owned(),
            position: pos(2, 2)
        }
    );
}

#[test]
fn second_root_is_not_allowed() {
    assert_eq!(
        parse_str("<a/> <b/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "second root element".to_owned(),
            position: pos(1, 6)
        }
    );
}

#[test]
fn xml_declaration_must_be_first() {
    assert_eq!(
        parse_str(" <?xml version=\"1.0\"?><a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "XML declaration".to_owned(),
            position: pos(1, 2)
        }
    );
    assert_eq!(
        parse_str("<?pi?><?xml version=\"1.0\"?><a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "XML declaration".to_owned(),
            position: pos(1, 7)
        }
    );
}

#[test]
fn max_depth() -> anyhow::Result<()> {
    let nested = |depth: usize| "<a>".repeat(depth) + &"</a>".repeat(depth);
    parse_str(&nested(MAX_DEPTH))?;
    match parse_str(&nested(MAX_DEPTH + 1)).unwrap_err().into_kind() {
        ErrorKind::MaxDepthExceeded {
            path,
            position,
            max_depth,
        } => {
            assert_eq!(max_depth, 256);
            assert_eq!(position, pos(1, 3 * 256 + 1));
            assert_eq!(path.matches("/a").count(), 256);
        }
        e => panic!("unexpected error {e:?}"),
    }
    Ok(())
}

#[test]
fn very_deep_document_does_not_overflow_stack() {
    let depth = 1_000_000;
    let input = "<a>".repeat(depth) + &"</a>".repeat(depth);
    assert_eq!(
        parse_str(&input).unwrap_err(),
        ErrorKind::MaxDepthExceeded {
            path: "/a".to_owned() + &"/a[1]".repeat(MAX_DEPTH - 1),
            position: pos(1, 3 * MAX_DEPTH as u32 + 1),
            max_depth: MAX_DEPTH
        }
    );
}

#[test]
fn missing_root_element() {
    assert_eq!(
        parse_str(r#"<?xml version="1.0" encoding="utf-8"?>"#).unwrap_err(),
        ErrorKind::Xml {
            message: "missing root element".to_owned(),
            position: pos(1, 39)
        }
    );
    assert_eq!(
        parse_str("").unwrap_err(),
        ErrorKind::Xml {
            message: "missing root element".to_owned(),
            position: pos(1, 1)
        }
    );
}

/// Asserts that parsing fails with [`ErrorKind::Xml`] and returns the reported position.
fn xml_error_position(input: &str) -> Position {
    match parse_str(input).unwrap_err().into_kind() {
        ErrorKind::Xml { position, .. } => position,
        e => panic!("expected malformed XML error, got {e:?}"),
    }
}

#[test]
fn different_closing_tag() {
    // Points to `</b>`.
    assert_eq!(xml_error_position("<root>\n  <a></b>\n</root>"), pos(2, 6));
}

#[test]
fn missing_closing_tag() {
    assert_eq!(
        parse_str("<root><person>").unwrap_err(),
        ErrorKind::Xml {
            message: "element `person` not closed".to_owned(),
            position: pos(1, 15)
        }
    );
}

#[test]
fn unclosed_comment() {
    assert_eq!(xml_error_position("<a>\n  <!-- x</a>"), pos(2, 3));
}

// ---------------------------------------------------------------------------
// Extraction errors (ported from UseData.Xml).
// ---------------------------------------------------------------------------

const TWO_NAMES: &str = "<person>\n  <name>Peter</name>\n  <name>James</name>\n</person>";

#[test]
fn multiple_children_when_single_child_is_expected() -> anyhow::Result<()> {
    let doc = parse_str(TWO_NAMES)?;
    let e = doc.root();
    assert_eq!(
        e.child(NO, "name").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/person".to_owned(),
            position: pos(1, 1),
            name: QualifiedName::new("", "name"),
            expected: "exactly one",
            found: 2
        }
    );
    Ok(())
}

#[test]
fn multiple_children_when_at_most_one_child_is_expected() -> anyhow::Result<()> {
    let doc = parse_str(TWO_NAMES)?;
    let e = doc.root();
    assert_eq!(
        e.child_opt(NO, "name").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/person".to_owned(),
            position: pos(1, 1),
            name: QualifiedName::new("", "name"),
            expected: "at most one",
            found: 2
        }
    );
    Ok(())
}

#[test]
fn no_child_when_one_child_is_expected() -> anyhow::Result<()> {
    let doc = parse_str("<dog></dog>")?;
    let e = doc.root();
    assert_eq!(
        e.child(NO, "name").unwrap_err(),
        ErrorKind::Missing {
            path: "/dog".to_owned(),
            position: pos(1, 1),
            item: child_item("", "name")
        }
    );
    Ok(())
}

#[test]
fn no_attribute_when_one_attribute_is_expected() -> anyhow::Result<()> {
    let doc = parse_str("<city></city>")?;
    let e = doc.root();
    assert_eq!(
        e.attr(NO, "name", parse::string).unwrap_err(),
        ErrorKind::Missing {
            path: "/city".to_owned(),
            position: pos(1, 1),
            item: attr_item("", "name")
        }
    );
    Ok(())
}

#[test]
fn invalid_value() -> anyhow::Result<()> {
    let doc = parse_str("<r>\n  <n v=\"x\">12a</n>\n</r>")?;
    let e = doc.root();
    let n = e.child(NO, "n")?;
    assert_eq!(
        n.attr(NO, "v", parse::u32).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/r/n[1]".to_owned(),
            position: pos(2, 3),
            item: attr_item("", "v"),
            message: r#"expected u32, got "x""#.to_owned()
        }
    );
    assert_eq!(
        n.text(parse::u32).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/r/n[1]".to_owned(),
            position: pos(2, 3),
            item: Item::Text,
            message: r#"expected u32, got "12a""#.to_owned()
        }
    );
    Ok(())
}

#[test]
fn custom_parser_with_any_display_error() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a n="7"/>"#)?;
    let e = doc.root();
    assert_eq!(e.attr(NO, "n", |s: String| s.parse::<u8>())?, 7);
    Ok(())
}

// ---------------------------------------------------------------------------
// Usage tracking.
// ---------------------------------------------------------------------------

fn already_used(path: &str, position: Position, item: Item) -> ErrorKind {
    ErrorKind::AlreadyUsed {
        path: path.to_owned(),
        position,
        item,
    }
}

#[test]
fn attribute_can_be_used_once() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a x="1"/>"#)?;
    let e = doc.root();
    e.attr(NO, "x", parse::string)?;
    assert_eq!(
        e.attr_opt(NO, "x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), attr_item("", "x"))
    );
    Ok(())
}

#[test]
fn missing_attribute_can_be_requested_once() -> anyhow::Result<()> {
    let doc = parse_str("<a/>")?;
    let e = doc.root();
    assert_eq!(e.attr_opt(NO, "x", parse::string)?, None);
    assert_eq!(
        e.attr_opt(NO, "x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), attr_item("", "x"))
    );
    // `attr` reports the missing attribute only for the first request.
    let doc = parse_str("<a/>")?;
    let e = doc.root();
    assert_eq!(
        e.attr(NO, "x", parse::string).unwrap_err(),
        ErrorKind::Missing {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: attr_item("", "x")
        }
    );
    assert_eq!(
        e.attr(NO, "x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), attr_item("", "x"))
    );
    Ok(())
}

#[test]
fn text_can_be_used_once() -> anyhow::Result<()> {
    let doc = parse_str("<a>t</a>")?;
    let e = doc.root();
    e.text(parse::string)?;
    assert_eq!(
        e.text(parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Text)
    );
    Ok(())
}

#[test]
fn children_can_be_used_once() -> anyhow::Result<()> {
    let doc = parse_str("<a><b/></a>")?;
    let e = doc.root();
    e.children(NO, "b")?;
    assert_eq!(
        e.child(NO, "b").unwrap_err(),
        already_used("/a", pos(1, 1), child_item("", "b"))
    );
    // Missing children are tracked too.
    assert_eq!(e.child_opt(NO, "c")?.map(|c| c.local_name()), None);
    assert_eq!(
        e.children(NO, "c").unwrap_err(),
        already_used("/a", pos(1, 1), child_item("", "c"))
    );
    Ok(())
}

#[test]
fn ignore_all_marks_everything_as_used() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a x="1"><b y="2">t</b></a>"#)?;
    let e = doc.root();
    e.ignore_all();
    e.check_all_used()?;
    assert_eq!(
        e.attr(NO, "x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), attr_item("", "x"))
    );
    assert_eq!(
        e.text(parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Text)
    );
    assert_eq!(
        e.children(NO, "b").unwrap_err(),
        already_used("/a", pos(1, 1), child_item("", "b"))
    );
    Ok(())
}

#[test]
fn ignore_all_on_parent_affects_previously_obtained_child() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a><b y="2"/></a>"#)?;
    let e = doc.root();
    let b = e.child(NO, "b")?;
    e.ignore_all();
    assert_eq!(
        b.attr(NO, "y", parse::string).unwrap_err(),
        already_used("/a/b[1]", pos(1, 4), attr_item("", "y"))
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn ignore_all_on_child() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a><b y="2"><c/></b><d/></a>"#)?;
    let e = doc.root();
    e.child(NO, "b")?.ignore_all();
    e.child(NO, "d")?;
    e.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// `check_all_used` (ported from UseData.Xml).
// ---------------------------------------------------------------------------

const COMPANY: &str = "<company>
    <address>
        <street>Baker Street</street>
        <city>London</city>
    </address>
</company>";

#[test]
fn unused_element() -> anyhow::Result<()> {
    let doc = parse_str(COMPANY)?;
    let e = doc.root();
    let street = e
        .child(NO, "address")?
        .child(NO, "street")?
        .text(parse::string)?;
    assert_eq!(street, "Baker Street");
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            // The path and the position of the unused child, not of its parent.
            path: "/company/address[1]/city[1]".to_owned(),
            position: pos(4, 9),
            item: child_item("", "city")
        }
    );
    Ok(())
}

#[test]
fn unused_child_points_to_the_first_child_with_the_name() -> anyhow::Result<()> {
    let doc = parse_str("<r>\n  <a/>\n  <b/>\n  <b/>\n</r>")?;
    let e = doc.root();
    e.children(NO, "a")?;
    let error = e.check_all_used().unwrap_err();
    assert_eq!(
        error,
        ErrorKind::Unused {
            path: "/r/b[1]".to_owned(),
            position: pos(3, 3),
            item: child_item("", "b")
        }
    );
    assert_eq!(
        error.to_string(),
        "element /r/b[1] at line 3, column 3 not used"
    );
    e.children(NO, "b")?;
    e.check_all_used()?;
    Ok(())
}

#[test]
fn error_position_and_path() -> anyhow::Result<()> {
    // Errors related to an element.
    let doc = parse_str("<r>\n  <a x=\"1\"/></r>")?;
    let e = doc.root();
    let a = e.child(NO, "a")?;
    let error = a.attr(NO, "y", parse::string).unwrap_err();
    assert_eq!(
        (error.path(), error.position()),
        (Some("/r/a[1]"), Some(pos(2, 3)))
    );
    assert_eq!(error.kind().path(), Some("/r/a[1]"));
    assert_eq!(error.kind().position(), Some(pos(2, 3)));
    let error = e.check_all_used().unwrap_err();
    assert_eq!(
        (error.path(), error.position()),
        (Some("/r/a[1]"), Some(pos(2, 3)))
    );

    // Errors not related to an element.
    let error = parse_str("<r>\n  </b>").unwrap_err();
    assert_eq!((error.path(), error.position()), (None, Some(pos(2, 3))));
    let error = parse_str("<a/>\n<b/>").unwrap_err();
    assert_eq!((error.path(), error.position()), (None, Some(pos(2, 1))));
    // The offending child or attribute.
    let error = parse_str("<r>\n<a x='1' x='2'/></r>").unwrap_err();
    assert_eq!(
        (error.path(), error.position()),
        (Some("/r/a[1]"), Some(pos(2, 10)))
    );
    let error = parse_str(&"<a>".repeat(MAX_DEPTH + 1)).unwrap_err();
    assert_eq!(error.position(), Some(pos(1, 3 * MAX_DEPTH as u32 + 1)));
    Ok(())
}

#[test]
fn unused_attribute() -> anyhow::Result<()> {
    let doc = parse_str(
        r#"<company>
    <address street="Baker Street" city="London" />
</company>"#,
    )?;
    let e = doc.root();
    assert_eq!(
        e.child(NO, "address")?.attr(NO, "city", parse::string)?,
        "London"
    );
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/company/address[1]".to_owned(),
            position: pos(2, 5),
            item: attr_item("", "street")
        }
    );
    Ok(())
}

#[test]
fn unused_text() -> anyhow::Result<()> {
    let doc = parse_str(COMPANY)?;
    let e = doc.root();
    let address = e.child(NO, "address")?;
    address.child(NO, "city")?;
    assert_eq!(
        address.child(NO, "street")?.text(parse::string)?,
        "Baker Street"
    );
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/company/address[1]/city[1]".to_owned(),
            position: pos(4, 9),
            item: Item::Text
        }
    );
    Ok(())
}

#[test]
fn check_order_attrs_then_text_then_children() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a z="1" y="2"><![CDATA[t]]></a>"#)?;
    let e = doc.root();
    let unused = |path: &str, position, item| ErrorKind::Unused {
        path: path.to_owned(),
        position,
        item,
    };
    // Attributes by the first occurrence of their names.
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), attr_item("", "z"))
    );
    e.attr(NO, "y", parse::string)?;
    e.attr(NO, "z", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), Item::Text)
    );
    e.text(parse::string)?;
    e.check_all_used()?;

    // Attributes before children, children by the first occurrence of their names.
    let doc = parse_str(r#"<a x="1"><b/><c/></a>"#)?;
    let e = doc.root();
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), attr_item("", "x"))
    );
    e.attr(NO, "x", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a/b[1]", pos(1, 10), child_item("", "b"))
    );
    // Own items before descendants.
    let doc = parse_str(r#"<a><b y="1"/><c/></a>"#)?;
    let e = doc.root();
    e.children(NO, "b")?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a/c[1]", pos(1, 14), child_item("", "c"))
    );
    Ok(())
}

#[test]
fn check_reports_first_unused_descendant_in_document_order() -> anyhow::Result<()> {
    let doc = parse_str(r#"<r><i/><i x="1"/><i y="1"/></r>"#)?;
    let e = doc.root();
    e.children(NO, "i")?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/i[2]".to_owned(),
            position: pos(1, 8),
            item: attr_item("", "x")
        }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Paths and positions.
// ---------------------------------------------------------------------------

#[test]
fn paths() -> anyhow::Result<()> {
    let doc = parse_str("<root><items><item/><x/><item/><item/></items></root>")?;
    let e = doc.root();
    assert_eq!(e.path(), "/root");
    let items = e.child(NO, "items")?;
    assert_eq!(items.path(), "/root/items[1]");
    assert_eq!(
        items.children(NO, "item")?[2].path(),
        "/root/items[1]/item[3]"
    );
    assert_eq!(items.child(NO, "x")?.path(), "/root/items[1]/x[1]");
    Ok(())
}

#[test]
fn positions_with_crlf_and_multibyte_characters() -> anyhow::Result<()> {
    let doc = parse_str("<r>\r\n<a v=\"č\"/><b/>\r\n\r\n  <č/></r>")?;
    let e = doc.root();
    assert_eq!(e.position(), pos(1, 1));
    assert_eq!(e.child(NO, "a")?.position(), pos(2, 1));
    assert_eq!(e.child(NO, "b")?.position(), pos(2, 11));
    assert_eq!(e.child(NO, "č")?.position(), pos(4, 3));
    Ok(())
}

// ---------------------------------------------------------------------------
// Rules without a test before the review.
// ---------------------------------------------------------------------------

#[test]
fn unknown_entity_in_attribute() {
    assert_eq!(
        parse_str("<r>\n  <a x=\"&nbsp;\"/></r>").unwrap_err(),
        ErrorKind::UnknownEntity {
            name: "nbsp".to_owned(),
            // Points to `&` in the attribute value.
            position: pos(2, 9)
        }
    );
}

#[test]
fn invalid_references_point_to_the_reference() {
    let xml = |message: &str, position| ErrorKind::Xml {
        message: message.to_owned(),
        position,
    };
    let not_closed = "entity or character reference not closed: `;` not found";
    let cases = [
        // In attribute values, also after other references and multibyte characters.
        (
            "<r>\n <a y='1' x='ab&#xZZ;'/></r>",
            xml("invalid character reference", pos(2, 16)),
        ),
        (
            "<a x='čč&amp;&#0;'/>",
            xml("character reference to a forbidden character", pos(1, 14)),
        ),
        ("<a x='a\r\nb &x'/>", xml(not_closed, pos(2, 3))),
        ("<a x='&amp'/>", xml(not_closed, pos(1, 7))),
        // In text.
        (
            "<r>\n <a>&#xZZ;</a></r>",
            xml("invalid character reference", pos(2, 5)),
        ),
        ("<a>a & b</a>", xml("invalid reference", pos(1, 6))),
        ("<r>\n <a>č&amp</a></r>", xml(not_closed, pos(2, 6))),
        ("<a>&</a>", xml("invalid reference", pos(1, 4))),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_str(input).unwrap_err(), expected, "{input:?}");
    }
}

#[test]
fn references_outside_of_root_are_not_allowed() {
    let reference = |position| ErrorKind::UnexpectedNode {
        node: "entity reference outside of the root element".to_owned(),
        position,
    };
    assert_eq!(parse_str("<a/>&amp;").unwrap_err(), reference(pos(1, 5)));
    // Even a reference to a whitespace character is not whitespace.
    assert_eq!(parse_str("&#32;<a/>").unwrap_err(), reference(pos(1, 1)));
    assert_eq!(parse_str("<a/>\n&#x20;").unwrap_err(), reference(pos(2, 1)));
    assert_eq!(parse_str("<a/>&nbsp;").unwrap_err(), reference(pos(1, 5)));
}

#[test]
fn cdata_outside_of_root_is_not_allowed() {
    let cdata = |position| ErrorKind::UnexpectedNode {
        node: "CDATA section outside of the root element".to_owned(),
        position,
    };
    assert_eq!(parse_str("<![CDATA[]]><a/>").unwrap_err(), cdata(pos(1, 1)));
    assert_eq!(
        parse_str("<a/>\n<![CDATA[ ]]>").unwrap_err(),
        cdata(pos(2, 1))
    );
}

#[test]
fn text_outside_of_root_points_to_first_non_whitespace_character() {
    let text = |position| ErrorKind::UnexpectedNode {
        node: "text outside of the root element".to_owned(),
        position,
    };
    assert_eq!(parse_str("  \n x<a/>").unwrap_err(), text(pos(2, 2)));
    assert_eq!(parse_str("<a/>\n\n   x").unwrap_err(), text(pos(3, 4)));
}

#[test]
fn doctype_and_declaration_after_root_are_not_allowed() {
    assert_eq!(
        parse_str("<a/>\n<!DOCTYPE a>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "DOCTYPE".to_owned(),
            position: pos(2, 1)
        }
    );
    assert_eq!(
        parse_str(r#"<a/><?xml version="1.0"?>"#).unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "XML declaration".to_owned(),
            position: pos(1, 5)
        }
    );
}

#[test]
fn only_one_byte_order_mark_is_skipped() {
    assert_eq!(
        parse_str("\u{FEFF}\u{FEFF}<a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "text outside of the root element".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn attribute_values_are_normalized() -> anyhow::Result<()> {
    // Literal tabs and line breaks become spaces, character references are kept.
    let doc = parse_str("<a x=\"a\tb\r\nc\rd\ne&#10;f&#9;g\"/>")?;
    let e = doc.root();
    assert_eq!(e.attr(NO, "x", parse::string)?, "a b c d e\nf\tg");
    Ok(())
}

#[test]
fn character_references_outside_of_bmp_are_decoded() -> anyhow::Result<()> {
    let doc = parse_str("<a>&#x1F600;&#128512;</a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "😀😀");
    Ok(())
}

#[test]
fn whitespace_only_cdata_must_be_used() -> anyhow::Result<()> {
    let doc = parse_str("<a><![CDATA[ ]]></a>")?;
    let e = doc.root();
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Text
        }
    );
    assert_eq!(e.text(parse::string)?, " ");
    e.check_all_used()?;

    // Whitespace-only text without CDATA does not need to be used.
    parse_str("<a> \n\t </a>")?.check_all_used()?;
    Ok(())
}

#[test]
fn max_depth_with_empty_element() -> anyhow::Result<()> {
    let nested = |depth: usize| "<a>".repeat(depth - 1) + "<a/>" + &"</a>".repeat(depth - 1);
    parse_str(&nested(MAX_DEPTH))?;
    match parse_str(&nested(MAX_DEPTH + 1)).unwrap_err().into_kind() {
        ErrorKind::MaxDepthExceeded {
            path,
            position,
            max_depth,
        } => {
            assert_eq!(max_depth, MAX_DEPTH);
            assert_eq!(position, pos(1, 3 * 256 + 1));
            assert_eq!(path, "/a".to_owned() + &"/a[1]".repeat(255));
        }
        e => panic!("unexpected error {e:?}"),
    }
    Ok(())
}

#[test]
fn positions_with_lone_carriage_returns() -> anyhow::Result<()> {
    let doc = parse_str("<r>\r<a/>\r\r<b/>\r\n\r<c/></r>")?;
    let e = doc.root();
    assert_eq!(e.child(NO, "a")?.position(), pos(2, 1));
    assert_eq!(e.child(NO, "b")?.position(), pos(4, 1));
    assert_eq!(e.child(NO, "c")?.position(), pos(6, 1));
    Ok(())
}

#[test]
fn positions_with_characters_outside_of_bmp() -> anyhow::Result<()> {
    // `😀` is one Unicode scalar value (four bytes in UTF-8, two UTF-16 code units).
    let doc = parse_str("<r><a x=\"😀\"/>\t<b/></r>")?;
    let e = doc.root();
    assert_eq!(e.child(NO, "a")?.position(), pos(1, 4));
    assert_eq!(e.child(NO, "b")?.position(), pos(1, 15));
    Ok(())
}

#[test]
fn missing_names_are_already_used_after_ignore_all() -> anyhow::Result<()> {
    let doc = parse_str("<a><b/></a>")?;
    let e = doc.root();
    let b = e.child(NO, "b")?;
    e.ignore_all();
    assert_eq!(
        e.attr_opt(NO, "x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), attr_item("", "x"))
    );
    assert_eq!(
        e.child_opt(NO, "y").unwrap_err(),
        already_used("/a", pos(1, 1), child_item("", "y"))
    );
    assert_eq!(
        b.children(NO, "z").unwrap_err(),
        already_used("/a/b[1]", pos(1, 4), child_item("", "z"))
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn ignore_all_on_child_does_not_hide_unused_siblings() -> anyhow::Result<()> {
    let doc = parse_str(r#"<a><b x="1"/><b y="2"/></a>"#)?;
    let e = doc.root();
    e.children(NO, "b")?[0].ignore_all();
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/a/b[2]".to_owned(),
            position: pos(1, 14),
            item: attr_item("", "y")
        }
    );
    Ok(())
}

#[test]
fn value_parser_can_use_the_same_element() -> anyhow::Result<()> {
    // Usage tracking must not be borrowed while a value parser runs.
    let doc = parse_str(r#"<a x="1" y="2"/>"#)?;
    let e = doc.root();
    let x = e.attr(NO, "x", |s| -> Result<String, use_xml::Error> {
        let y = e.attr(NO, "y", parse::string)?;
        assert!(e.attr_opt(NO, "missing", parse::string)?.is_none());
        assert_eq!(
            e.attr(NO, "x", parse::string).unwrap_err(),
            already_used("/a", pos(1, 1), attr_item("", "x"))
        );
        Ok(format!("{s}{y}"))
    })?;
    assert_eq!(x, "12");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn text_parser_can_use_the_same_element_and_document() -> anyhow::Result<()> {
    // Usage tracking and the names must not be borrowed while a value parser runs.
    let doc = parse_str(r#"<a x="1">t</a>"#)?;
    let e = doc.root();
    let text = e.text(|s| -> Result<String, use_xml::Error> {
        assert!(format!("{e:?}").contains("text: <used>"));
        let ns = e.document().namespace("urn:new")?;
        assert!(e.children(ns, "b")?.is_empty());
        assert_eq!(e.attr(NO, "x", parse::u8)?, 1);
        assert_eq!(
            e.text(parse::string).unwrap_err(),
            already_used("/a", pos(1, 1), Item::Text)
        );
        e.check_all_used()?;
        Ok(s)
    })?;
    assert_eq!(text, "t");
    doc.check_all_used()?;
    Ok(())
}

#[test]
fn error_kind_accessors() {
    let error = parse_str("<a><b/>x</a>").unwrap_err();
    let expected = ErrorKind::MixedContent {
        path: "/a".to_owned(),
        position: pos(1, 8),
    };
    assert_eq!(error.kind(), &expected);
    assert_eq!(error, expected);
    assert_eq!(Error::from(expected.clone()), error);
    assert_eq!(
        error.to_string(),
        "element /a contains mixed content at line 1, column 8"
    );
    assert_eq!(format!("{error:?}"), format!("{expected:?}"));
    assert_eq!(error.into_kind(), expected);
}

#[test]
fn requesting_child_name_marks_all_children_as_used() -> anyhow::Result<()> {
    let doc = parse_str("<a><b/><b/></a>")?;
    let e = doc.root();
    assert_eq!(
        e.child(NO, "b").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/a".to_owned(),
            position: pos(1, 1),
            name: QualifiedName::new("", "b"),
            expected: "exactly one",
            found: 2
        }
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn line_endings_in_cdata_are_normalized() -> anyhow::Result<()> {
    let doc = parse_str("<a><![CDATA[x\r\ny\rz]]></a>")?;
    let e = doc.root();
    assert_eq!(e.text(parse::string)?, "x\ny\nz");
    Ok(())
}

// ---------------------------------------------------------------------------
// `Debug` and long values.
// ---------------------------------------------------------------------------

#[test]
fn debug_prints_only_the_element() -> anyhow::Result<()> {
    let doc = parse_str(r#"<r x="1"><a y="2"><b/></a><a/></r>"#)?;
    let e = doc.root();
    assert_eq!(
        format!("{e:?}"),
        r#"Elem { path: "/r", position: Position { line: 1, column: 1 }, attrs: {"x": "1"}, children: {"a": 2}, text: "" }"#
    );
    // `Debug` does not use anything.
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r".to_owned(),
            position: pos(1, 1),
            item: attr_item("", "x")
        }
    );
    Ok(())
}

#[test]
fn debug_shows_used_and_missing_items() -> anyhow::Result<()> {
    let doc = parse_str(r#"<r x="1">t</r>"#)?;
    let e = doc.root();
    e.attr(NO, "x", parse::u8)?;
    assert_eq!(e.attr_opt(NO, "y", parse::u8)?, None);
    e.text(parse::string)?;
    assert_eq!(
        format!("{e:?}"),
        r#"Elem { path: "/r", position: Position { line: 1, column: 1 }, attrs: {"x": <used or missing>, "y": <used or missing>}, children: {}, text: <used> }"#
    );
    Ok(())
}

#[test]
fn debug_of_deep_document_is_small() -> anyhow::Result<()> {
    let doc = parse_str(&("<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH)))?;
    let e = doc.root();
    assert!(format!("{e:#?}").len() < 200);
    Ok(())
}

#[test]
fn debug_cuts_long_values() -> anyhow::Result<()> {
    let long = "č".repeat(MAX_VALUE_CHARS + 1);
    let doc = parse_str(&format!(r#"<a x="{long}">{long}</a>"#))?;
    let e = doc.root();
    let cut = format!("{}…", "č".repeat(MAX_VALUE_CHARS));
    assert_eq!(
        format!("{e:?}"),
        format!(
            r#"Elem {{ path: "/a", position: Position {{ line: 1, column: 1 }}, attrs: {{"x": "{cut}"}}, children: {{}}, text: "{cut}" }}"#
        )
    );
    Ok(())
}

#[test]
fn invalid_value_cuts_long_values() -> anyhow::Result<()> {
    let exact = "x".repeat(MAX_VALUE_CHARS);
    let long = "ž".repeat(MAX_VALUE_CHARS + 1);
    let doc = parse_str(&format!(r#"<a x="{exact}">{long}</a>"#))?;
    let e = doc.root();
    assert_eq!(
        e.attr(NO, "x", parse::u8).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: attr_item("", "x"),
            message: format!("expected u8, got {exact:?}")
        }
    );
    assert_eq!(
        e.text(parse::u8).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Text,
            message: format!("expected u8, got \"{}…\"", "ž".repeat(MAX_VALUE_CHARS))
        }
    );
    Ok(())
}

#[test]
fn mixed_content_whitespace_cdata_before_child() {
    // CDATA is significant even if it contains only whitespace.
    assert_eq!(
        parse_str("<r>\n<a><![CDATA[ ]]><b/></a></r>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/r/a[1]".to_owned(),
            position: pos(2, 4)
        }
    );
}

#[test]
fn document_is_send() -> anyhow::Result<()> {
    // `Document` is `Send` but not `Sync` (checked by a `compile_fail` doc test in `lib.rs`).
    let doc = parse_str("<a/>")?;
    let name = std::thread::spawn(move || doc.root().local_name().to_owned())
        .join()
        .map_err(|_| anyhow::anyhow!("thread panicked"))?;
    assert_eq!(name, "a");
    Ok(())
}
