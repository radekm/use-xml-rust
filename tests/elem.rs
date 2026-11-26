use use_xml::{Error, ErrorKind, Item, MAX_DEPTH, MAX_VALUE_CHARS, Position, parse, parse_str};

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
}

// ---------------------------------------------------------------------------
// Basic usage (ported from UseData.Xml).
// ---------------------------------------------------------------------------

#[test]
fn xml_declaration() -> anyhow::Result<()> {
    let e = parse_str(r#"<?xml version="1.0" encoding="utf-8"?><animals />"#)?;
    assert_eq!(e.name(), "animals");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn child_elements() -> anyhow::Result<()> {
    let e = parse_str(
        r#"
    <animals>
        <dog>A</dog>
        <pig>B</pig>
        <dog>D</dog>
    </animals>
    "#,
    )?;
    let dogs = e
        .children("dog")?
        .iter()
        .map(|d| d.text(parse::string))
        .collect::<Result<Vec<_>, _>>()?;
    let pig = e.child("pig")?.text(parse::string)?;
    let cat = e.child_opt("cat")?;
    assert_eq!(dogs, ["A", "D"]);
    assert_eq!(pig, "B");
    assert!(cat.is_none());
    e.check_all_used()?;
    Ok(())
}

#[test]
fn attributes() -> anyhow::Result<()> {
    let e = parse_str(
        r#"
    <people>
        <person name="Pete" />
        <person name="Harry" address="Little Whinging" />
    </people>
    "#,
    )?;
    let people = e
        .children("person")?
        .iter()
        .map(|p| {
            Ok((
                p.attr("name", parse::string)?,
                p.attr_opt("address", parse::string)?,
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
    let e = parse_str(
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
    let cell = e.child("table")?.child("tr")?.child("td")?;
    assert_eq!(cell.text(parse::string)?, "Cell");
    // `xmlns:h` is not an attribute.
    e.check_all_used()?;
    Ok(())
}

#[test]
fn default_namespace() -> anyhow::Result<()> {
    let e = parse_str(
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
    let cell = e.child("table")?.child("tr")?.child("td")?;
    assert_eq!(cell.text(parse::string)?, "Cell");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn prefixed_attributes_use_local_name() -> anyhow::Result<()> {
    let e = parse_str(r#"<a xmlns:p="urn:p" p:x="1" />"#)?;
    assert_eq!(e.attr("x", parse::i32)?, 1);
    e.check_all_used()?;
    Ok(())
}

#[test]
fn children_with_same_prefix_are_allowed() -> anyhow::Result<()> {
    let e = parse_str(r#"<r xmlns:a="urn:a"><a:x/><a:x/><x/></r>"#);
    // `a:x` and `x` differ in prefix.
    assert_eq!(
        e.unwrap_err(),
        ErrorKind::ChildPrefixMismatch {
            path: "/r".to_owned(),
            position: pos(1, 32),
            name: "x".to_owned(),
            first_prefix: "a".to_owned(),
            second_prefix: String::new(),
        }
    );
    let e = parse_str(r#"<r xmlns:a="urn:a"><a:x/><a:x/></r>"#)?;
    assert_eq!(e.children("x")?.len(), 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// Text content.
// ---------------------------------------------------------------------------

#[test]
fn whitespace_text_is_kept_without_children() -> anyhow::Result<()> {
    let e = parse_str("<a>  \n </a>")?;
    // Whitespace-only text does not need to be used.
    e.check_all_used()?;
    assert_eq!(e.text(parse::string)?, "  \n ");
    Ok(())
}

#[test]
fn text_of_element_with_children_is_empty() -> anyhow::Result<()> {
    let e = parse_str("<a>\n  <b/>\n  <c/>\n</a>")?;
    assert_eq!(e.text(parse::string)?, "");
    Ok(())
}

#[test]
fn text_of_empty_element_is_empty() -> anyhow::Result<()> {
    let e = parse_str("<r><a/><b></b></r>")?;
    assert_eq!(e.child("a")?.text(parse::string)?, "");
    assert_eq!(e.child("b")?.text(parse::string)?, "");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn cdata_and_text_are_concatenated() -> anyhow::Result<()> {
    let e = parse_str("<a> x <![CDATA[<y>]]> z </a>")?;
    assert_eq!(e.text(parse::string)?, " x <y> z ");
    Ok(())
}

#[test]
fn empty_cdata_is_significant() -> anyhow::Result<()> {
    let e = parse_str("<a><![CDATA[]]></a>")?;
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
    let e = parse_str(r#"<a x="&lt;&#65;&quot;">&amp;&#x42;&apos;&gt;</a>"#)?;
    assert_eq!(e.attr("x", parse::string)?, "<A\"");
    assert_eq!(e.text(parse::string)?, "&B'>");
    Ok(())
}

#[test]
fn whitespace_char_reference_is_not_significant() -> anyhow::Result<()> {
    let e = parse_str("<a>&#32;<b/></a>")?;
    assert_eq!(e.text(parse::string)?, "");
    Ok(())
}

#[test]
fn comments_and_processing_instructions_are_ignored() -> anyhow::Result<()> {
    let e = parse_str("<!-- c --><?pi x?><a>x<!-- c -->y<?pi?></a><!-- c -->")?;
    assert_eq!(e.text(parse::string)?, "xy");
    Ok(())
}

#[test]
fn line_endings_are_normalized() -> anyhow::Result<()> {
    let e = parse_str("<a>x\r\ny\rz</a>")?;
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
            position: pos(1, 1)
        }
    );
}

#[test]
fn mixed_content_text_after_child() {
    assert_eq!(
        parse_str("<r><a><b/>text</a></r>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/r/a[1]".to_owned(),
            position: pos(1, 4)
        }
    );
}

#[test]
fn mixed_content_cdata_after_child() {
    assert_eq!(
        parse_str("<r>\n<a><b/><![CDATA[]]></a></r>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/r/a[1]".to_owned(),
            position: pos(2, 1)
        }
    );
}

#[test]
fn mixed_content_entity_reference_after_child() {
    assert_eq!(
        parse_str("<a><b/>&amp;</a>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/a".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn mixed_content_entity_reference_before_child() {
    assert_eq!(
        parse_str("<a>&#65;<b/></a>").unwrap_err(),
        ErrorKind::MixedContent {
            path: "/a".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn duplicate_attribute_with_different_prefixes() {
    assert_eq!(
        parse_str(
            r#"<r xmlns:a="urn:a" xmlns:b="urn:b">
<e a:x="1" b:x="2"/></r>"#
        )
        .unwrap_err(),
        ErrorKind::DuplicateAttr {
            path: "/r/e[1]".to_owned(),
            position: pos(2, 1),
            name: "x".to_owned()
        }
    );
}

#[test]
fn duplicate_attribute_with_same_name() {
    assert_eq!(
        parse_str(r#"<e x="1" x="2"/>"#).unwrap_err(),
        ErrorKind::DuplicateAttr {
            path: "/e".to_owned(),
            position: pos(1, 1),
            name: "x".to_owned()
        }
    );
}

#[test]
fn child_prefix_mismatch() {
    assert_eq!(
        parse_str(r#"<r xmlns:a="urn:a" xmlns:b="urn:b"><a:c/><b:c/></r>"#).unwrap_err(),
        ErrorKind::ChildPrefixMismatch {
            path: "/r".to_owned(),
            position: pos(1, 42),
            name: "c".to_owned(),
            first_prefix: "a".to_owned(),
            second_prefix: "b".to_owned(),
        }
    );
}

#[test]
fn unknown_entity() {
    assert_eq!(
        parse_str("<a>x &nbsp;</a>").unwrap_err(),
        ErrorKind::UnknownEntity {
            name: "nbsp".to_owned(),
            path: "/a".to_owned(),
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
            message: "unexpected end of document".to_owned(),
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
    let e = parse_str(TWO_NAMES)?;
    assert_eq!(
        e.child("name").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/person".to_owned(),
            position: pos(1, 1),
            name: "name".to_owned(),
            expected: "exactly one",
            found: 2
        }
    );
    Ok(())
}

#[test]
fn multiple_children_when_at_most_one_child_is_expected() -> anyhow::Result<()> {
    let e = parse_str(TWO_NAMES)?;
    assert_eq!(
        e.child_opt("name").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/person".to_owned(),
            position: pos(1, 1),
            name: "name".to_owned(),
            expected: "at most one",
            found: 2
        }
    );
    Ok(())
}

#[test]
fn no_child_when_one_child_is_expected() -> anyhow::Result<()> {
    let e = parse_str("<dog></dog>")?;
    assert_eq!(
        e.child("name").unwrap_err(),
        ErrorKind::Missing {
            path: "/dog".to_owned(),
            position: pos(1, 1),
            item: Item::Child("name".to_owned())
        }
    );
    Ok(())
}

#[test]
fn no_attribute_when_one_attribute_is_expected() -> anyhow::Result<()> {
    let e = parse_str("<city></city>")?;
    assert_eq!(
        e.attr("name", parse::string).unwrap_err(),
        ErrorKind::Missing {
            path: "/city".to_owned(),
            position: pos(1, 1),
            item: Item::Attr("name".to_owned())
        }
    );
    Ok(())
}

#[test]
fn invalid_value() -> anyhow::Result<()> {
    let e = parse_str("<r>\n  <n v=\"x\">12a</n>\n</r>")?;
    let n = e.child("n")?;
    assert_eq!(
        n.attr("v", parse::u32).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/r/n[1]".to_owned(),
            position: pos(2, 3),
            item: Item::Attr("v".to_owned()),
            value: "x".to_owned(),
            message: "expected u32".to_owned()
        }
    );
    assert_eq!(
        n.text(parse::u32).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/r/n[1]".to_owned(),
            position: pos(2, 3),
            item: Item::Text,
            value: "12a".to_owned(),
            message: "expected u32".to_owned()
        }
    );
    Ok(())
}

#[test]
fn custom_parser_with_any_display_error() -> anyhow::Result<()> {
    let e = parse_str(r#"<a n="7"/>"#)?;
    assert_eq!(e.attr("n", str::parse::<u8>)?, 7);
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
    let e = parse_str(r#"<a x="1"/>"#)?;
    e.attr("x", parse::string)?;
    assert_eq!(
        e.attr_opt("x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    Ok(())
}

#[test]
fn missing_attribute_can_be_requested_once() -> anyhow::Result<()> {
    let e = parse_str("<a/>")?;
    assert_eq!(e.attr_opt("x", parse::string)?, None);
    assert_eq!(
        e.attr_opt("x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    // `attr` reports the missing attribute only for the first request.
    let e = parse_str("<a/>")?;
    assert_eq!(
        e.attr("x", parse::string).unwrap_err(),
        ErrorKind::Missing {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Attr("x".to_owned())
        }
    );
    assert_eq!(
        e.attr("x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    Ok(())
}

#[test]
fn text_can_be_used_once() -> anyhow::Result<()> {
    let e = parse_str("<a>t</a>")?;
    e.text(parse::string)?;
    assert_eq!(
        e.text(parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Text)
    );
    Ok(())
}

#[test]
fn children_can_be_used_once() -> anyhow::Result<()> {
    let e = parse_str("<a><b/></a>")?;
    e.children("b")?;
    assert_eq!(
        e.child("b").unwrap_err(),
        already_used("/a", pos(1, 1), Item::Child("b".to_owned()))
    );
    // Missing children are tracked too.
    assert_eq!(e.child_opt("c")?.map(|c| c.name()), None);
    assert_eq!(
        e.children("c").unwrap_err(),
        already_used("/a", pos(1, 1), Item::Child("c".to_owned()))
    );
    Ok(())
}

#[test]
fn ignore_all_marks_everything_as_used() -> anyhow::Result<()> {
    let e = parse_str(r#"<a x="1"><b y="2">t</b></a>"#)?;
    e.ignore_all();
    e.check_all_used()?;
    assert_eq!(
        e.attr("x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    assert_eq!(
        e.text(parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Text)
    );
    assert_eq!(
        e.children("b").unwrap_err(),
        already_used("/a", pos(1, 1), Item::Child("b".to_owned()))
    );
    Ok(())
}

#[test]
fn ignore_all_on_parent_affects_previously_obtained_child() -> anyhow::Result<()> {
    let e = parse_str(r#"<a><b y="2"/></a>"#)?;
    let b = e.child("b")?;
    e.ignore_all();
    assert_eq!(
        b.attr("y", parse::string).unwrap_err(),
        already_used("/a/b[1]", pos(1, 4), Item::Attr("y".to_owned()))
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn ignore_all_on_child() -> anyhow::Result<()> {
    let e = parse_str(r#"<a><b y="2"><c/></b><d/></a>"#)?;
    e.child("b")?.ignore_all();
    e.child("d")?;
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
    let e = parse_str(COMPANY)?;
    let street = e.child("address")?.child("street")?.text(parse::string)?;
    assert_eq!(street, "Baker Street");
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            // The path and the position of the unused child, not of its parent.
            path: "/company/address[1]/city[1]".to_owned(),
            position: pos(4, 9),
            item: Item::Child("city".to_owned())
        }
    );
    Ok(())
}

#[test]
fn unused_child_points_to_the_first_child_with_the_name() -> anyhow::Result<()> {
    let e = parse_str("<r>\n  <b/>\n  <a/>\n  <a/>\n</r>")?;
    let error = e.check_all_used().unwrap_err();
    assert_eq!(
        error,
        ErrorKind::Unused {
            path: "/r/a[1]".to_owned(),
            position: pos(3, 3),
            item: Item::Child("a".to_owned())
        }
    );
    assert_eq!(
        error.to_string(),
        "element /r/a[1] at line 3, column 3 not used"
    );
    e.children("a")?;
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "element /r/b[1] at line 2, column 3 not used"
    );
    Ok(())
}

#[test]
fn error_position_and_path() -> anyhow::Result<()> {
    // Errors related to an element.
    let e = parse_str("<r>\n  <a x=\"1\"/></r>")?;
    let a = e.child("a")?;
    let error = a.attr("y", parse::string).unwrap_err();
    assert_eq!(
        (error.path(), error.position()),
        (Some("/r/a[1]"), pos(2, 3))
    );
    assert_eq!(error.kind().path(), Some("/r/a[1]"));
    assert_eq!(error.kind().position(), pos(2, 3));
    let error = e.check_all_used().unwrap_err();
    assert_eq!(
        (error.path(), error.position()),
        (Some("/r/a[1]"), pos(2, 3))
    );

    // Errors not related to an element.
    let error = parse_str("<r>\n  </b>").unwrap_err();
    assert_eq!((error.path(), error.position()), (None, pos(2, 3)));
    let error = parse_str("<a/>\n<b/>").unwrap_err();
    assert_eq!((error.path(), error.position()), (None, pos(2, 1)));

    // The offending child.
    let error = parse_str("<r xmlns:p=\"u\"><c/>\n<p:c/></r>").unwrap_err();
    assert_eq!((error.path(), error.position()), (Some("/r"), pos(2, 1)));
    Ok(())
}

#[test]
fn unused_attribute() -> anyhow::Result<()> {
    let e = parse_str(
        r#"<company>
    <address street="Baker Street" city="London" />
</company>"#,
    )?;
    assert_eq!(e.child("address")?.attr("city", parse::string)?, "London");
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/company/address[1]".to_owned(),
            position: pos(2, 5),
            item: Item::Attr("street".to_owned())
        }
    );
    Ok(())
}

#[test]
fn unused_text() -> anyhow::Result<()> {
    let e = parse_str(COMPANY)?;
    let address = e.child("address")?;
    address.child("city")?;
    assert_eq!(
        address.child("street")?.text(parse::string)?,
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
    let e = parse_str(r#"<a z="1" y="2"><![CDATA[t]]></a>"#)?;
    let unused = |path: &str, position, item| ErrorKind::Unused {
        path: path.to_owned(),
        position,
        item,
    };
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), Item::Attr("y".to_owned()))
    );
    e.attr("y", parse::string)?;
    e.attr("z", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), Item::Text)
    );
    e.text(parse::string)?;
    e.check_all_used()?;

    // Attributes before children, children in name order.
    let e = parse_str(r#"<a x="1"><c/><b/></a>"#)?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    e.attr("x", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a/b[1]", pos(1, 14), Item::Child("b".to_owned()))
    );
    // Own items before descendants.
    let e = parse_str(r#"<a><b y="1"/><c/></a>"#)?;
    e.children("b")?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        unused("/a/c[1]", pos(1, 14), Item::Child("c".to_owned()))
    );
    Ok(())
}

#[test]
fn check_reports_first_unused_descendant_in_document_order() -> anyhow::Result<()> {
    let e = parse_str(r#"<r><i/><i x="1"/><i y="1"/></r>"#)?;
    e.children("i")?;
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/i[2]".to_owned(),
            position: pos(1, 8),
            item: Item::Attr("x".to_owned())
        }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Paths and positions.
// ---------------------------------------------------------------------------

#[test]
fn paths() -> anyhow::Result<()> {
    let e = parse_str("<root><items><item/><x/><item/><item/></items></root>")?;
    assert_eq!(e.path(), "/root");
    let items = e.child("items")?;
    assert_eq!(items.path(), "/root/items[1]");
    assert_eq!(items.children("item")?[2].path(), "/root/items[1]/item[3]");
    assert_eq!(items.child("x")?.path(), "/root/items[1]/x[1]");
    Ok(())
}

#[test]
fn positions_with_crlf_and_multibyte_characters() -> anyhow::Result<()> {
    let e = parse_str("<r>\r\n<a v=\"č\"/><b/>\r\n\r\n  <č/></r>")?;
    assert_eq!(e.position(), pos(1, 1));
    assert_eq!(e.child("a")?.position(), pos(2, 1));
    assert_eq!(e.child("b")?.position(), pos(2, 11));
    assert_eq!(e.child("č")?.position(), pos(4, 3));
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
            path: "/r/a[1]".to_owned(),
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
            xml(
                "invalid character reference: invalid digit found in string",
                pos(2, 16),
            ),
        ),
        (
            "<a x='čč&amp;&#0;'/>",
            xml(
                "invalid character reference: 0x0 character is not permitted in XML",
                pos(1, 14),
            ),
        ),
        ("<a x='a\r\nb &x'/>", xml(not_closed, pos(2, 3))),
        ("<a x='&amp'/>", xml(not_closed, pos(1, 7))),
        // In text.
        (
            "<r>\n <a>&#xZZ;</a></r>",
            xml(
                "invalid character reference: invalid digit found in string",
                pos(2, 5),
            ),
        ),
        // The same message as in attribute values, although the input continues.
        ("<a>a & b</a>", xml(not_closed, pos(1, 6))),
        ("<r>\n <a>č&amp</a></r>", xml(not_closed, pos(2, 6))),
        ("<a>&</a>", xml(not_closed, pos(1, 4))),
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
fn attribute_syntax_errors_have_exact_positions() {
    assert_eq!(
        parse_str("<a x=1/>").unwrap_err(),
        ErrorKind::Xml {
            message: "attribute value must be enclosed in `\"` or `'`".to_owned(),
            position: pos(1, 6)
        }
    );
    assert_eq!(
        parse_str("<a\n  x y='1'/>").unwrap_err(),
        ErrorKind::Xml {
            message: "attribute name must be directly followed by `=` or whitespace".to_owned(),
            position: pos(2, 5)
        }
    );
    assert_eq!(
        parse_str("<a x= />").unwrap_err(),
        ErrorKind::Xml {
            message: "`=` must be followed by an attribute value".to_owned(),
            position: pos(1, 7)
        }
    );
}

#[test]
fn attribute_values_are_normalized() -> anyhow::Result<()> {
    // Literal tabs and line breaks become spaces, character references are kept.
    let e = parse_str("<a x=\"a\tb\r\nc\rd\ne&#10;f&#9;g\"/>")?;
    assert_eq!(e.attr("x", parse::string)?, "a b c d e\nf\tg");
    Ok(())
}

#[test]
fn character_references_outside_of_bmp_are_decoded() -> anyhow::Result<()> {
    let e = parse_str("<a>&#x1F600;&#128512;</a>")?;
    assert_eq!(e.text(parse::string)?, "😀😀");
    Ok(())
}

#[test]
fn whitespace_only_cdata_must_be_used() -> anyhow::Result<()> {
    let e = parse_str("<a><![CDATA[ ]]></a>")?;
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
    let e = parse_str("<r>\r<a/>\r\r<b/>\r\n\r<c/></r>")?;
    assert_eq!(e.child("a")?.position(), pos(2, 1));
    assert_eq!(e.child("b")?.position(), pos(4, 1));
    assert_eq!(e.child("c")?.position(), pos(6, 1));
    Ok(())
}

#[test]
fn positions_with_characters_outside_of_bmp() -> anyhow::Result<()> {
    // `😀` is one Unicode scalar value (four bytes in UTF-8, two UTF-16 code units).
    let e = parse_str("<r><a x=\"😀\"/>\t<b/></r>")?;
    assert_eq!(e.child("a")?.position(), pos(1, 4));
    assert_eq!(e.child("b")?.position(), pos(1, 15));
    Ok(())
}

#[test]
fn missing_names_are_already_used_after_ignore_all() -> anyhow::Result<()> {
    let e = parse_str("<a><b/></a>")?;
    let b = e.child("b")?;
    e.ignore_all();
    assert_eq!(
        e.attr_opt("x", parse::string).unwrap_err(),
        already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
    );
    assert_eq!(
        e.child_opt("y").unwrap_err(),
        already_used("/a", pos(1, 1), Item::Child("y".to_owned()))
    );
    assert_eq!(
        b.children("z").unwrap_err(),
        already_used("/a/b[1]", pos(1, 4), Item::Child("z".to_owned()))
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn ignore_all_on_child_does_not_hide_unused_siblings() -> anyhow::Result<()> {
    let e = parse_str(r#"<a><b x="1"/><b y="2"/></a>"#)?;
    e.children("b")?[0].ignore_all();
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/a/b[2]".to_owned(),
            position: pos(1, 14),
            item: Item::Attr("y".to_owned())
        }
    );
    Ok(())
}

#[test]
fn value_parser_can_use_the_same_element() -> anyhow::Result<()> {
    // Usage tracking must not be borrowed while a value parser runs.
    let e = parse_str(r#"<a x="1" y="2"/>"#)?;
    let x = e.attr("x", |s| -> Result<String, use_xml::Error> {
        let y = e.attr("y", parse::string)?;
        assert!(e.attr_opt("missing", parse::string)?.is_none());
        assert_eq!(
            e.attr("x", parse::string).unwrap_err(),
            already_used("/a", pos(1, 1), Item::Attr("x".to_owned()))
        );
        Ok(format!("{s}{y}"))
    })?;
    assert_eq!(x, "12");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn error_kind_accessors() {
    let error = parse_str("<a><b/>x</a>").unwrap_err();
    let expected = ErrorKind::MixedContent {
        path: "/a".to_owned(),
        position: pos(1, 1),
    };
    assert_eq!(error.kind(), &expected);
    assert_eq!(error, expected);
    assert_eq!(Error::from(expected.clone()), error);
    assert_eq!(
        error.to_string(),
        "element /a at line 1, column 1 contains mixed content"
    );
    assert_eq!(format!("{error:?}"), format!("{expected:?}"));
    assert_eq!(error.into_kind(), expected);
}

#[test]
fn child_prefix_mismatch_with_unprefixed_child() {
    assert_eq!(
        parse_str("<r xmlns:a=\"urn:a\">\n  <c/>\n  <a:c/></r>").unwrap_err(),
        ErrorKind::ChildPrefixMismatch {
            path: "/r".to_owned(),
            position: pos(3, 3),
            name: "c".to_owned(),
            first_prefix: String::new(),
            second_prefix: "a".to_owned(),
        }
    );
}

#[test]
fn requesting_child_name_marks_all_children_as_used() -> anyhow::Result<()> {
    let e = parse_str("<a><b/><b/></a>")?;
    assert_eq!(
        e.child("b").unwrap_err(),
        ErrorKind::WrongCount {
            path: "/a".to_owned(),
            position: pos(1, 1),
            name: "b".to_owned(),
            expected: "exactly one",
            found: 2
        }
    );
    e.check_all_used()?;
    Ok(())
}

#[test]
fn line_endings_in_cdata_are_normalized() -> anyhow::Result<()> {
    let e = parse_str("<a><![CDATA[x\r\ny\rz]]></a>")?;
    assert_eq!(e.text(parse::string)?, "x\ny\nz");
    Ok(())
}

// ---------------------------------------------------------------------------
// `Debug` and long values.
// ---------------------------------------------------------------------------

#[test]
fn debug_prints_only_the_element() -> anyhow::Result<()> {
    let e = parse_str(r#"<r x="1"><a y="2"><b/></a><a/></r>"#)?;
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
            item: Item::Attr("x".to_owned())
        }
    );
    Ok(())
}

#[test]
fn debug_of_deep_document_is_small() -> anyhow::Result<()> {
    let e = parse_str(&("<a>".repeat(MAX_DEPTH) + &"</a>".repeat(MAX_DEPTH)))?;
    assert!(format!("{e:#?}").len() < 200);
    Ok(())
}

#[test]
fn debug_cuts_long_values() -> anyhow::Result<()> {
    let long = "č".repeat(MAX_VALUE_CHARS + 1);
    let e = parse_str(&format!(r#"<a x="{long}">{long}</a>"#))?;
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
    let e = parse_str(&format!(r#"<a x="{exact}">{long}</a>"#))?;
    assert_eq!(
        e.attr("x", parse::u8).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Attr("x".to_owned()),
            value: exact,
            message: "expected u8".to_owned()
        }
    );
    assert_eq!(
        e.text(parse::u8).unwrap_err(),
        ErrorKind::InvalidValue {
            path: "/a".to_owned(),
            position: pos(1, 1),
            item: Item::Text,
            value: format!("{}…", "ž".repeat(MAX_VALUE_CHARS)),
            message: "expected u8".to_owned()
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
            position: pos(2, 1)
        }
    );
}

#[test]
fn elem_is_send() -> anyhow::Result<()> {
    // `Elem` is `Send` but not `Sync` (checked by a `compile_fail` doc test in `lib.rs`).
    fn assert_send<T: Send>(_: &T) {}
    let e = parse_str("<a/>")?;
    assert_send(&e);
    let name = std::thread::spawn(move || e.name().to_owned())
        .join()
        .map_err(|_| anyhow::anyhow!("thread panicked"))?;
    assert_eq!(name, "a");
    Ok(())
}
