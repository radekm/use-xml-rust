//! Traversal and dropping of deep documents with a small stack.

use use_xml::{ErrorKind, Item, MAX_DEPTH, Position, parse_str};

fn pos(line: u32, column: u32) -> Position {
    Position { line, column }
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
