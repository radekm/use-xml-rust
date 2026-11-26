use use_xml::{Error, ErrorKind, Item, MAX_DEPTH, Position, parse, parse_str};

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
    assert!(matches!(
        e.map_err(Error::into_kind),
        Err(ErrorKind::ChildPrefixMismatch { .. })
    ));
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
    assert!(matches!(
        parse_str("<a><b/><![CDATA[]]></a>").map_err(Error::into_kind),
        Err(ErrorKind::MixedContent { .. })
    ));
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
    assert!(matches!(
        parse_str("x<a/>").map_err(Error::into_kind),
        Err(ErrorKind::UnexpectedNode {
            position: Position { line: 1, column: 1 },
            ..
        })
    ));
}

#[test]
fn text_after_root_is_not_allowed() {
    assert!(matches!(
        parse_str("<a/>\n x").map_err(Error::into_kind),
        Err(ErrorKind::UnexpectedNode {
            position: Position { line: 2, column: 2 },
            ..
        })
    ));
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
    assert!(matches!(
        parse_str(" <?xml version=\"1.0\"?><a/>").map_err(Error::into_kind),
        Err(ErrorKind::UnexpectedNode { .. }) | Err(ErrorKind::Xml { .. })
    ));
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
    assert!(matches!(
        parse_str(&input).map_err(Error::into_kind),
        Err(ErrorKind::MaxDepthExceeded { .. })
    ));
}

#[test]
fn missing_root_element() {
    assert!(matches!(
        parse_str(r#"<?xml version="1.0" encoding="utf-8"?>"#).map_err(Error::into_kind),
        Err(ErrorKind::Xml { .. })
    ));
    assert!(matches!(
        parse_str("").map_err(Error::into_kind),
        Err(ErrorKind::Xml { .. })
    ));
}

#[test]
fn different_closing_tag() {
    assert!(matches!(
        parse_str("<root>\n  <a></b>\n</root>").map_err(Error::into_kind),
        Err(ErrorKind::Xml {
            position: Position { line: 2, .. },
            ..
        })
    ));
}

#[test]
fn missing_closing_tag() {
    assert!(matches!(
        parse_str("<root><person>").map_err(Error::into_kind),
        Err(ErrorKind::Xml { .. })
    ));
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
    assert!(matches!(
        e.child_opt("name").map_err(Error::into_kind),
        Err(ErrorKind::WrongCount {
            expected: "at most one",
            found: 2,
            ..
        })
    ));
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
    assert!(matches!(
        n.text(parse::u32).map_err(Error::into_kind),
        Err(ErrorKind::InvalidValue {
            item: Item::Text,
            ..
        })
    ));
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
    assert!(matches!(
        e.attr_opt("x", parse::string).map_err(Error::into_kind),
        Err(ErrorKind::AlreadyUsed { .. })
    ));
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
    assert!(matches!(
        e.children("c").map_err(Error::into_kind),
        Err(ErrorKind::AlreadyUsed { .. })
    ));
    Ok(())
}

#[test]
fn ignore_all_marks_everything_as_used() -> anyhow::Result<()> {
    let e = parse_str(r#"<a x="1"><b y="2">t</b>text-less</a>"#);
    assert!(e.is_err()); // Mixed content.

    let e = parse_str(r#"<a x="1"><b y="2">t</b></a>"#)?;
    e.ignore_all();
    e.check_all_used()?;
    assert!(matches!(
        e.attr("x", parse::string).map_err(Error::into_kind),
        Err(ErrorKind::AlreadyUsed { .. })
    ));
    assert!(matches!(
        e.text(parse::string).map_err(Error::into_kind),
        Err(ErrorKind::AlreadyUsed { .. })
    ));
    assert!(matches!(
        e.children("b").map_err(Error::into_kind),
        Err(ErrorKind::AlreadyUsed { .. })
    ));
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
            path: "/company/address[1]".to_owned(),
            position: pos(2, 5),
            item: Item::Child("city".to_owned())
        }
    );
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
    assert!(matches!(
        e.check_all_used().map_err(Error::into_kind),
        Err(ErrorKind::Unused { item: Item::Attr(ref n), .. }) if n == "y"
    ));
    e.attr("y", parse::string)?;
    e.attr("z", parse::string)?;
    assert!(matches!(
        e.check_all_used().map_err(Error::into_kind),
        Err(ErrorKind::Unused {
            item: Item::Text,
            ..
        })
    ));
    e.text(parse::string)?;
    e.check_all_used()?;

    let e = parse_str("<a><c/><b/></a>")?;
    assert!(matches!(
        e.check_all_used().map_err(Error::into_kind),
        Err(ErrorKind::Unused { item: Item::Child(ref n), .. }) if n == "b"
    ));
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
