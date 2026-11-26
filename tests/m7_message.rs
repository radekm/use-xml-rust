#[path = "../benches/m7.rs"]
mod m7;

#[test]
fn m7_message() {
    let deltas = m7::parse_message(m7::MESSAGE).expect("message is valid");
    assert_eq!(deltas.len(), 14);
    let first = &deltas[0];
    assert_eq!(first.contract_id, 1790055);
    assert_eq!(first.area_id, "10YDE-EON------1");
    assert_eq!(first.revision, 1973);
    assert!(first.sell.is_empty());
    assert_eq!(first.buy.len(), 1);
    assert_eq!(first.buy[0].order_id, 505982385);
    assert_eq!(first.buy[0].price, 57);
    assert_eq!(
        first.buy[0].entry_time,
        time::macros::datetime!(2022-11-12 10:02:54.553 UTC)
    );
}
