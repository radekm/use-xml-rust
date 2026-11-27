#![allow(dead_code)] // The fields are read only in tests.

//! Extraction of an M7 order book delta report.
//!
//! The message format is described in "DFS180 - M7 - Public Message Interface":
//! <https://www.semopx.com/documents/general-publications/DFS180-API-Specifications-M7-6.13-v1.0.pdf>
//!
//! Ported from the benchmark of UseData.Xml.

use time::OffsetDateTime;
use use_xml::{Elem, Error, NamespaceHandle, parse};

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

/// Namespaces of the message.
#[derive(Clone, Copy)]
struct Ns {
    m7: NamespaceHandle,
    no: NamespaceHandle,
}

fn parse_orders(ns: Ns, list: Option<Elem<'_>>) -> Result<Vec<Order>, Error> {
    let Some(list) = list else {
        return Ok(Vec::new());
    };
    list.children(ns.m7, "OrdrBookEntry")?
        .into_iter()
        .map(|e| {
            Ok(Order {
                order_id: e.attr(ns.no, "ordrId", parse::u64)?,
                price: e.attr(ns.no, "px", parse::i64)?,
                quantity: e.attr(ns.no, "qty", parse::u32)?,
                entry_time: e.attr(ns.no, "ordrEntryTime", parse::date_time_utc)?,
            })
        })
        .collect()
}

fn parse_order_book_delta(ns: Ns, root: Elem<'_>) -> Result<Vec<OrderBookDelta>, Error> {
    root.child(ns.m7, "StandardHeader")?
        .attr(ns.no, "marketId", parse::string)?;
    let Some(list) = root.child_opt(ns.m7, "OrdrbookList")? else {
        return Ok(Vec::new());
    };
    list.children(ns.m7, "OrdrBook")?
        .into_iter()
        .map(|e| {
            e.attr_opt(ns.no, "lastPx", parse::i64)?;
            e.attr_opt(ns.no, "lastQty", parse::u32)?;
            e.attr_opt(ns.no, "totalQty", parse::u32)?;
            e.attr_opt(ns.no, "lastTradeTime", parse::date_time_utc)?;
            e.attr_opt(ns.no, "pxDir", parse::i32)?;
            e.attr_opt(ns.no, "highPx", parse::i64)?;
            e.attr_opt(ns.no, "lowPx", parse::i64)?;
            Ok(OrderBookDelta {
                contract_id: e.attr(ns.no, "contractId", parse::u64)?,
                area_id: e.attr(ns.no, "dlvryAreaId", parse::string_non_whitespace)?,
                revision: e.attr(ns.no, "revisionNo", parse::u32)?,
                buy: parse_orders(ns, e.child_opt(ns.m7, "BuyOrdrList")?)?,
                sell: parse_orders(ns, e.child_opt(ns.m7, "SellOrdrList")?)?,
            })
        })
        .collect()
}

/// Parses the message, extracts the data, and checks that everything was used.
pub fn parse_message(message: &str) -> Result<Vec<OrderBookDelta>, Error> {
    let doc = use_xml::parse_str(message)?;
    let ns = Ns {
        m7: doc.namespace("http://www.deutsche-boerse.com/m7/v6")?,
        no: doc.namespace("")?,
    };
    let result = parse_order_book_delta(ns, doc.root())?;
    doc.check_all_used()?;
    Ok(result)
}
