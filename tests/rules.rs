//! Tests of documented rules and limitations which are not covered elsewhere,
//! the traversal of deep documents with a small stack, and the messages of all errors.

use use_xml::{ErrorKind, Item, MAX_DEPTH, Position, parse, parse_bytes, parse_str};

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
}

// ---------------------------------------------------------------------------
// Whitespace.
// ---------------------------------------------------------------------------

#[test]
fn non_xml_whitespace_is_significant() -> anyhow::Result<()> {
    // Only space, tab, carriage return, and line feed are XML whitespace.
    for ws in ["\u{A0}", "\u{2003}", "\u{3000}"] {
        let e = parse_str(&format!("<r>\n<a>{ws}</a></r>"))?;
        e.child("a")?;
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
// Namespaces.
// ---------------------------------------------------------------------------

#[test]
fn namespace_declarations_are_not_checked() -> anyhow::Result<()> {
    // By design: `xmlns` and `xmlns:*` are skipped before any other check,
    // so neither references in their values nor duplicates are detected.
    let e = parse_str("<a xmlns='&foo;' xmlns:p='&#0;' xmlns:q='&amp' x='1'/>")?;
    assert_eq!(e.attr("x", parse::string)?, "1");
    e.check_all_used()?;

    let e = parse_str("<a xmlns='1' xmlns='2' xmlns:p='1' xmlns:p='2'/>")?;
    assert_eq!(e.attr_opt("xmlns", parse::string)?, None);
    assert_eq!(e.attr_opt("p", parse::string)?, None);
    e.check_all_used()?;

    // The same values in other attributes are errors.
    assert_eq!(
        parse_str("<a\n x='&foo;'/>").unwrap_err(),
        ErrorKind::UnknownEntity {
            name: "foo".to_owned(),
            path: "/a".to_owned(),
            position: pos(2, 5)
        }
    );
    assert_eq!(
        parse_str("<a x='1'\n x='2'/>").unwrap_err(),
        ErrorKind::DuplicateAttr {
            path: "/a".to_owned(),
            position: pos(1, 1),
            name: "x".to_owned()
        }
    );
    Ok(())
}

#[test]
fn xml_prefixed_attributes_collide_with_unprefixed_ones() {
    for (input, name) in [
        (r#"<r><a xml:lang="cs" lang="en"/></r>"#, "lang"),
        (r#"<r><a space="x" xml:space="preserve"/></r>"#, "space"),
    ] {
        assert_eq!(
            parse_str(input).unwrap_err(),
            ErrorKind::DuplicateAttr {
                path: "/r/a[1]".to_owned(),
                position: pos(1, 4),
                name: name.to_owned()
            },
            "{input}"
        );
    }
}

#[test]
fn xml_prefixed_attribute_is_found_by_local_name() -> anyhow::Result<()> {
    let e = parse_str(r#"<a xml:lang="cs"/>"#)?;
    assert_eq!(e.attr("lang", parse::string)?, "cs");
    e.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Known limitations of `quick-xml`.
// ---------------------------------------------------------------------------

#[test]
fn empty_element_names_are_accepted() -> anyhow::Result<()> {
    for input in ["< />", "<a:/>"] {
        let e = parse_str(input)?;
        assert_eq!(e.name(), "", "{input}");
        assert_eq!(e.path(), "/", "{input}");
    }

    let e = parse_str("<r><b/>\n< /></r>")?;
    let empty = e.child("")?;
    assert_eq!(empty.path(), "/r/[1]");
    assert_eq!(empty.position(), pos(2, 1));
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/b[1]".to_owned(),
            position: pos(1, 4),
            item: Item::Child("b".to_owned())
        }
    );
    Ok(())
}

#[test]
fn missing_whitespace_between_attributes_is_accepted() -> anyhow::Result<()> {
    let e = parse_str(r#"<a x="1"y="2"/>"#)?;
    assert_eq!(e.attr("x", parse::u8)?, 1);
    assert_eq!(e.attr("y", parse::u8)?, 2);
    e.check_all_used()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Deep documents with a small stack.
// ---------------------------------------------------------------------------

/// Stack size of the threads below. A recursive `check_all_used` or `Drop` of `Elem` needed
/// hundreds of kilobytes of stack for a document nested `MAX_DEPTH` levels deep in debug builds,
/// a recursive `Drop` of `ElemPath` more than 32 KiB. The iterative versions fit into 16 KiB.
const SMALL_STACK: usize = 32 * 1024;

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
fn check_all_used_of_deep_document_needs_little_stack() -> anyhow::Result<()> {
    let root = parse_str(&deepest_document())?;
    let mut elem = &root;
    while let Some(child) = elem.child_opt("a")? {
        elem = child;
    }
    run_with_small_stack(move || {
        assert_eq!(
            root.check_all_used().unwrap_err(),
            ErrorKind::Unused {
                path: "/a".to_owned() + &"/a[1]".repeat(MAX_DEPTH - 1),
                position: pos(1, 3 * MAX_DEPTH as u32 - 2),
                item: Item::Attr("x".to_owned())
            }
        );
    });
    Ok(())
}

#[test]
fn drop_of_deep_document_needs_little_stack() -> anyhow::Result<()> {
    let root = parse_str(&deepest_document())?;
    run_with_small_stack(move || drop(root));
    Ok(())
}

#[test]
fn drop_of_deep_unfinished_document_needs_little_stack() {
    // Parsing fails at the end and the paths of all open elements are dropped.
    let input = "<a>".repeat(MAX_DEPTH);
    run_with_small_stack(move || {
        assert_eq!(
            parse_str(&input).unwrap_err(),
            ErrorKind::Xml {
                message: "unexpected end of document".to_owned(),
                position: pos(1, 3 * MAX_DEPTH as u32 + 1)
            }
        );
    });
}

#[test]
fn check_all_used_goes_deep_before_next_name() -> anyhow::Result<()> {
    let e = parse_str("<r><b y='2'/><a><c z='3'/></a><a w='4'/></r>")?;
    e.children("a")?[0].child("c")?;
    e.child("b")?;
    // `a` before `b` (by name), the first `a` with its descendants before the second `a`.
    assert_eq!(
        e.check_all_used().unwrap_err(),
        ErrorKind::Unused {
            path: "/r/a[1]/c[1]".to_owned(),
            position: pos(1, 17),
            item: Item::Attr("z".to_owned())
        }
    );
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
        "malformed XML at line 2, column 4: ill-formed document: expected `</b>`, but `</c>` was found"
    );
    assert_eq!(
        parse_bytes(b"<?xml version='1.0' encoding='latin1'?><a/>")
            .unwrap_err()
            .to_string(),
        "unsupported encoding at line 1, column 1: encoding `latin1` is not supported"
    );
    assert_eq!(
        message("<!DOCTYPE a><a/>"),
        "unexpected DOCTYPE at line 1, column 1"
    );
    assert_eq!(
        message("<a>\n&foo;</a>"),
        "unknown entity `foo` in element /a at line 2, column 1"
    );
    assert_eq!(
        message("<a><b/>x</a>"),
        "element /a at line 1, column 1 contains mixed content"
    );
    assert_eq!(
        message("<a p:x='1' q:x='2'/>"),
        "element /a at line 1, column 1 contains multiple attributes with local name `x`"
    );
    assert_eq!(
        message("<r><p:a/>\n<a/></r>"),
        "element /r contains children with local name `a` but different prefixes `p` and `` \
         (second one at line 2, column 1)"
    );
    assert_eq!(
        message(&"<a>".repeat(MAX_DEPTH + 1)),
        format!(
            "element /a{} contains a child at line 1, column {} which exceeds the maximum depth 256",
            "/a[1]".repeat(MAX_DEPTH - 1),
            3 * MAX_DEPTH + 1
        )
    );

    let e = parse_str("<r x='a'><b/><b/></r>")?;
    assert_eq!(
        e.attr("y", parse::string).unwrap_err().to_string(),
        "element /r at line 1, column 1 has no attribute `y`"
    );
    assert_eq!(
        e.child("c").unwrap_err().to_string(),
        "element /r at line 1, column 1 has no child `c`"
    );
    assert_eq!(
        e.child("b").unwrap_err().to_string(),
        "element /r at line 1, column 1 has 2 children `b`, expected exactly one"
    );
    assert_eq!(
        e.child_opt("b").unwrap_err().to_string(),
        "child `b` of element /r at line 1, column 1 already used"
    );
    assert_eq!(
        e.attr("x", parse::u8).unwrap_err().to_string(),
        r#"invalid attribute `x` of element /r at line 1, column 1: expected u8 (value: "a")"#
    );
    // `child` marked `b` as used even though it failed.
    e.check_all_used()?;

    let e = parse_str("<r><b/><b/></r>")?;
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "element /r/b[1] at line 1, column 4 not used"
    );
    assert_eq!(
        e.child_opt("b").unwrap_err().to_string(),
        "element /r at line 1, column 1 has 2 children `b`, expected at most one"
    );

    let e = parse_str("<r x='1'>t</r>")?;
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "attribute `x` of element /r at line 1, column 1 not used"
    );
    e.attr("x", parse::string)?;
    assert_eq!(
        e.check_all_used().unwrap_err().to_string(),
        "text of element /r at line 1, column 1 not used"
    );
    assert_eq!(
        e.text(parse::bool).unwrap_err().to_string(),
        r#"invalid text of element /r at line 1, column 1: expected bool (value: "t")"#
    );
    assert_eq!(
        e.text(parse::string).unwrap_err().to_string(),
        "text of element /r at line 1, column 1 already used"
    );
    Ok(())
}
