use use_xml::{ErrorKind, Position, parse, parse_bytes, parse_str};

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
}

#[test]
fn utf8_bytes() -> anyhow::Result<()> {
    let e = parse_bytes("<a x=\"č\">ř</a>".as_bytes())?;
    assert_eq!(e.attr("x", parse::string)?, "č");
    assert_eq!(e.text(parse::string)?, "ř");
    e.check_all_used()?;
    Ok(())
}

#[test]
fn utf8_declaration_is_accepted() -> anyhow::Result<()> {
    parse_bytes(br#"<?xml version="1.0" encoding="UTF-8"?><a/>"#)?;
    parse_bytes(br#"<?xml version="1.0" encoding="utf-8"?><a/>"#)?;
    parse_bytes(br#"<?xml version="1.0"?><a/>"#)?;
    Ok(())
}

#[test]
fn utf8_bom_is_skipped() -> anyhow::Result<()> {
    let e = parse_bytes(b"\xEF\xBB\xBF<a>\n<b/></a>")?;
    assert_eq!(e.position(), pos(1, 1));
    assert_eq!(e.child("b")?.position(), pos(2, 1));
    let e = parse_str("\u{FEFF}<a/>")?;
    assert_eq!(e.name(), "a");
    Ok(())
}

#[test]
fn other_declared_encoding_is_rejected() {
    assert_eq!(
        parse_bytes(b"<?xml version=\"1.0\" encoding=\"windows-1250\"?><a/>").unwrap_err(),
        ErrorKind::Encoding {
            message: "encoding `windows-1250` is not supported".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn declared_encoding_is_ignored_for_str() -> anyhow::Result<()> {
    parse_str(r#"<?xml version="1.0" encoding="utf-16"?><a/>"#)?;
    Ok(())
}

#[test]
fn utf16_is_rejected() {
    for input in [
        // With a byte order mark.
        &b"\xFF\xFE<\0a\0/\0>\0"[..],
        &b"\xFE\xFF\0<\0a\0/\0>"[..],
        // Without a byte order mark.
        &b"<\0a\0/\0>\0"[..],
        &b"\0<\0a\0/\0>"[..],
    ] {
        assert_eq!(
            parse_bytes(input).unwrap_err(),
            ErrorKind::Encoding {
                message: "UTF-16 is not supported".to_owned(),
                position: pos(1, 1)
            },
            "{input:?}"
        );
    }
}

#[test]
fn utf32_is_rejected() {
    for input in [
        // With a byte order mark.
        &b"\xFF\xFE\0\0<\0\0\0"[..],
        &b"\0\0\xFE\xFF\0\0\0<"[..],
        // Without a byte order mark.
        &b"<\0\0\0a\0\0\0"[..],
        &b"\0\0\0<\0\0\0a"[..],
    ] {
        assert_eq!(
            parse_bytes(input).unwrap_err(),
            ErrorKind::Encoding {
                message: "UTF-32 is not supported".to_owned(),
                position: pos(1, 1)
            },
            "{input:?}"
        );
    }
}

#[test]
fn only_one_utf8_bom_is_skipped() {
    assert_eq!(
        parse_bytes(b"\xEF\xBB\xBF\xEF\xBB\xBF<a/>").unwrap_err(),
        ErrorKind::UnexpectedNode {
            node: "text outside of the root element".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn declared_encoding_is_checked_after_bom() {
    assert_eq!(
        parse_bytes(b"\xEF\xBB\xBF<?xml version='1.0' encoding='ISO-8859-2'?><a/>").unwrap_err(),
        ErrorKind::Encoding {
            message: "encoding `ISO-8859-2` is not supported".to_owned(),
            position: pos(1, 1)
        }
    );
}

#[test]
fn invalid_utf8_position_after_bom_and_line_breaks() {
    assert_eq!(
        parse_bytes(b"\xEF\xBB\xBF<a>\r\n\r\xC3\xA9\xFF</a>").unwrap_err(),
        ErrorKind::Encoding {
            message: "invalid UTF-8".to_owned(),
            position: pos(3, 2)
        }
    );
}

#[test]
fn invalid_utf8_is_rejected() {
    assert_eq!(
        parse_bytes(b"<a>\n  x\xFFy</a>").unwrap_err(),
        ErrorKind::Encoding {
            message: "invalid UTF-8".to_owned(),
            position: pos(2, 4)
        }
    );
}
