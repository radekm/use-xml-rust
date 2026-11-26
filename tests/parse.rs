use rust_decimal::dec;
use time::macros::datetime;
use use_xml::parse;

#[test]
fn strings() {
    assert_eq!(parse::string(" a "), Ok(" a ".to_owned()));
    assert_eq!(parse::string(""), Ok(String::new()));
    assert!(parse::string_non_empty("").is_err());
    assert_eq!(parse::string_non_empty(" "), Ok(" ".to_owned()));
    assert!(parse::string_non_whitespace(" \t\r\n").is_err());
    assert_eq!(parse::string_non_whitespace(" x "), Ok(" x ".to_owned()));
    // Only XML whitespace counts as whitespace.
    assert_eq!(
        parse::string_non_whitespace(" \u{A0} "),
        Ok(" \u{A0} ".to_owned())
    );
}

#[test]
fn one_of_and_enumeration() {
    let p = parse::one_of(&["a", "b"]);
    assert_eq!(p("b"), Ok("b".to_owned()));
    assert!(p("c").is_err());

    let p = parse::enumeration(&[("yes", true), ("no", false)]);
    assert_eq!(p("no"), Ok(false));
    assert_eq!(
        p("maybe"),
        Err(r#"expected one of ["yes", "no"]"#.to_owned())
    );
}

#[test]
fn integers() {
    assert_eq!(parse::i32("4532"), Ok(4532));
    assert_eq!(parse::i32("+4532"), Ok(4532));
    assert_eq!(parse::i32("-4532"), Ok(-4532));
    assert_eq!(parse::u64("18446744073709551615"), Ok(u64::MAX));
    assert!(parse::u8("256").is_err());
    assert!(parse::u32("-1").is_err());
    // Unsigned types accept only `+`, signed types accept both signs.
    assert_eq!(parse::u32("+7"), Ok(7));
    assert!(parse::u32("-0").is_err());
    assert_eq!(parse::i32("-0"), Ok(0));
    assert_eq!(parse::u8("007"), Ok(7));
    assert!(parse::i32(" 1").is_err());
    assert!(parse::i32("1 ").is_err());
    assert!(parse::i32("").is_err());
}

#[test]
fn decimal_parses_leading_sign() {
    assert_eq!(parse::decimal("+4532"), Ok(dec!(4532)));
    assert_eq!(parse::decimal("-4532"), Ok(dec!(-4532)));
}

#[test]
fn decimal_parses_dot() {
    assert_eq!(parse::decimal("4532.72"), Ok(dec!(4532.72)));
    assert_eq!(parse::decimal("0.1"), Ok(dec!(0.1)));
    assert_eq!(parse::decimal("00.03"), Ok(dec!(0.03)));
    assert_eq!(parse::decimal("0.0"), Ok(dec!(0)));
}

#[test]
fn decimal_is_strict() {
    for s in [
        "4532,7", "1.", ".5", "1e5", " 1", "1 ", "", "+", "-.1", "1_000", "1.2.3", "++1",
    ] {
        assert!(parse::decimal(s).is_err(), "{s:?} should be rejected");
    }
    // Too many digits to represent exactly.
    assert!(parse::decimal("0.12345678901234567890123456789012").is_err());
    assert!(parse::decimal(&format!("0.{}1", "0".repeat(28))).is_err());
    assert!(parse::decimal("79228162514264337593543950336.0").is_err());
}

#[test]
fn decimal_keeps_scale() {
    let parsed = parse::decimal("12.500").unwrap();
    assert_eq!(
        (parsed.to_string(), parsed.scale()),
        ("12.500".to_owned(), 3)
    );
    // 28 digits after `.` is the maximal scale.
    let max_scale = format!("1.{}", "0".repeat(28));
    assert_eq!(parse::decimal(&max_scale).unwrap().to_string(), max_scale);
}

#[test]
fn decimal_removes_trailing_zeros_only_when_needed() {
    let cases = [
        (format!("1.{}", "0".repeat(29)), "1"),
        (format!("-1.5{}", "0".repeat(30)), "-1.5"),
        (
            "79228162514264337593543950335.000".to_owned(),
            "79228162514264337593543950335",
        ),
        (
            "+79228162514264337593543950335.0".to_owned(),
            "79228162514264337593543950335",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            parse::decimal(&input).map(|d| d.to_string()),
            Ok(expected.to_owned()),
            "{input:?}"
        );
    }
}

#[test]
fn decimal_non_negative() {
    assert_eq!(parse::decimal_non_negative("0"), Ok(dec!(0)));
    assert_eq!(parse::decimal_non_negative("-0"), Ok(dec!(0)));
    assert!(parse::decimal_non_negative("-0.01").is_err());
}

#[test]
fn bool() {
    assert_eq!(parse::bool("true"), Ok(true));
    assert_eq!(parse::bool("false"), Ok(false));
    for s in ["True", "1", "0", " true", ""] {
        assert!(parse::bool(s).is_err(), "{s:?} should be rejected");
    }
}

#[test]
fn date_time_utc_without_fractional_seconds() {
    assert_eq!(
        parse::date_time_utc("2021-05-31T23:12:04Z"),
        Ok(datetime!(2021-05-31 23:12:04 UTC))
    );
}

#[test]
fn date_time_utc_with_fractional_seconds() {
    assert_eq!(
        parse::date_time_utc("2021-05-31T23:12:04.999Z"),
        Ok(datetime!(2021-05-31 23:12:04.999 UTC))
    );
    assert_eq!(
        parse::date_time_utc("2022-07-25T06:00:00.4Z"),
        Ok(datetime!(2022-07-25 06:00:00.4 UTC))
    );
    assert_eq!(
        parse::date_time_utc("2022-07-25T06:00:00.1340144Z"),
        Ok(datetime!(2022-07-25 06:00:00.1340144 UTC))
    );
    assert_eq!(
        parse::date_time_utc("2022-07-25T06:00:00.123456789Z"),
        Ok(datetime!(2022-07-25 06:00:00.123456789 UTC))
    );
}

#[test]
fn date_time_utc_with_space_separator() {
    assert_eq!(
        parse::date_time_utc("2021-05-31 23:12:04Z"),
        Ok(datetime!(2021-05-31 23:12:04 UTC))
    );
}

#[test]
fn date_time_utc_does_not_lose_information() {
    let format = time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:7]Z"
    );
    let s = "2022-07-25T06:01:02.1340144Z";
    let parsed = parse::date_time_utc(s).unwrap();
    assert_eq!(parsed.format(format).unwrap(), s);
}

#[test]
fn date_time_utc_failures() {
    for s in [
        "2021-13-31 23:12:04Z",
        "2021-02-29T00:00:00Z",
        "2021-05-31T24:00:00Z",
        "2021-05-31T23:12:04",
        "2021-05-31T23:12:04.Z",
        "2021-05-31T23:12:04.1234567890Z",
        "2021-05-31T23:12:04+00:00",
        "2021-05-31T23:12:04z",
        "2021-05-31X23:12:04Z",
        " 2021-05-31T23:12:04Z",
        "2021-5-31T23:12:04Z",
        "2021-05-31T23:12:04ZZ",
        "2021-05-31T23:12:04Zčč",
        "",
    ] {
        assert!(parse::date_time_utc(s).is_err(), "{s:?} should be rejected");
    }
}

#[test]
fn date_time_offset() {
    assert_eq!(
        parse::date_time_offset("2021-05-31T23:12:04Z"),
        Ok(datetime!(2021-05-31 23:12:04 UTC))
    );
    assert_eq!(
        parse::date_time_offset("2021-05-31T23:12:04.5+02:00"),
        Ok(datetime!(2021-05-31 23:12:04.5 +02:00))
    );
    assert_eq!(
        parse::date_time_offset("2021-05-31 23:12:04-05:30"),
        Ok(datetime!(2021-05-31 23:12:04 -05:30))
    );
    for s in [
        "2021-05-31T23:12:04",
        "2021-05-31T23:12:04+0200",
        "2021-05-31T23:12:04+02",
        "2021-05-31T23:12:04+24:00",
        "2021-05-31T23:12:04+02:60",
        "2021-05-31T23:12:04 +02:00",
    ] {
        assert!(
            parse::date_time_offset(s).is_err(),
            "{s:?} should be rejected"
        );
    }
}

#[test]
fn map_and_validate() {
    let half = parse::map(parse::u32, |n| {
        if n % 2 == 0 {
            Ok(n / 2)
        } else {
            Err("expected even number".to_owned())
        }
    });
    assert_eq!(half("8"), Ok(4));
    assert_eq!(half("7"), Err("expected even number".to_owned()));
    assert!(half("x").is_err());

    let small = parse::validate(parse::u32, |n| (*n > 10).then(|| "too big".to_owned()));
    assert_eq!(small("10"), Ok(10));
    assert_eq!(small("11"), Err("too big".to_owned()));
}

#[test]
fn date_time_offset_must_be_convertible_to_utc() {
    // The UTC equivalent of these date times is outside of the range of `time`,
    // `OffsetDateTime::to_offset(UtcOffset::UTC)` would panic on them.
    for s in ["9999-12-31T23:59:59-00:01", "9999-12-31T23:59:59-23:59"] {
        assert!(
            parse::date_time_offset(s).is_err(),
            "{s:?} should be rejected"
        );
    }
    let latest = parse::date_time_offset("9999-12-31T23:59:59+01:00").unwrap();
    assert_eq!(
        latest.to_offset(time::UtcOffset::UTC),
        datetime!(9999-12-31 22:59:59 UTC)
    );
    let earliest = parse::date_time_offset("0000-01-01T00:00:00-01:00").unwrap();
    assert_eq!(
        earliest.to_offset(time::UtcOffset::UTC),
        datetime!(0000-01-01 01:00:00 UTC)
    );
}
