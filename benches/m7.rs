#![allow(dead_code)] // The fields are read only in tests.

//! Extraction of an M7 order book delta report.
//!
//! The message format is described in "DFS180 - M7 - Public Message Interface":
//! <https://www.semopx.com/documents/general-publications/DFS180-API-Specifications-M7-6.13-v1.0.pdf>
//!
//! Ported from the benchmark of UseData.Xml.

use time::OffsetDateTime;
use use_xml::{Elem, Error, parse};

pub const MESSAGE: &str = include_str!("data/m7_message.xml");

#[derive(Debug)]
pub struct Order {
    pub order_id: u64,
    pub price: i64,
    pub quantity: u32,
    pub entry_time: OffsetDateTime,
}

#[derive(Debug)]
pub struct OrderBookDelta {
    pub contract_id: u64,
    pub area_id: String,
    pub revision: u32,
    pub buy: Vec<Order>,
    pub sell: Vec<Order>,
}

fn parse_orders(list: Option<&Elem>) -> Result<Vec<Order>, Error> {
    let Some(list) = list else {
        return Ok(Vec::new());
    };
    list.children("OrdrBookEntry")?
        .iter()
        .map(|e| {
            Ok(Order {
                order_id: e.attr("ordrId", parse::u64)?,
                price: e.attr("px", parse::i64)?,
                quantity: e.attr("qty", parse::u32)?,
                entry_time: e.attr("ordrEntryTime", parse::date_time_utc)?,
            })
        })
        .collect()
}

pub fn parse_order_book_delta(root: &Elem) -> Result<Vec<OrderBookDelta>, Error> {
    root.child("StandardHeader")?
        .attr("marketId", parse::string)?;
    let Some(list) = root.child_opt("OrdrbookList")? else {
        return Ok(Vec::new());
    };
    list.children("OrdrBook")?
        .iter()
        .map(|e| {
            e.attr_opt("lastPx", parse::i64)?;
            e.attr_opt("lastQty", parse::u32)?;
            e.attr_opt("totalQty", parse::u32)?;
            e.attr_opt("lastTradeTime", parse::date_time_utc)?;
            e.attr_opt("pxDir", parse::i32)?;
            e.attr_opt("highPx", parse::i64)?;
            e.attr_opt("lowPx", parse::i64)?;
            Ok(OrderBookDelta {
                contract_id: e.attr("contractId", parse::u64)?,
                area_id: e.attr("dlvryAreaId", parse::string_non_whitespace)?,
                revision: e.attr("revisionNo", parse::u32)?,
                buy: parse_orders(e.child_opt("BuyOrdrList")?)?,
                sell: parse_orders(e.child_opt("SellOrdrList")?)?,
            })
        })
        .collect()
}

/// Parses the message, extracts the data, and checks that everything was used.
pub fn parse_message() -> Result<Vec<OrderBookDelta>, Error> {
    let root = use_xml::parse_str(MESSAGE)?;
    let result = parse_order_book_delta(&root)?;
    root.check_all_used()?;
    Ok(result)
}
