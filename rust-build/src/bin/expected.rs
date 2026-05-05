//! Brute-force k-NN(=5) reference implementation for the smoke test.
//!
//! Computes the expected `fraud_count` for each payload in
//! `resources/example-payloads.json` by:
//!  1. loading every reference vector from `resources/references.json.gz`
//!     (or, when `RINHA_USE_EXAMPLE_REFS=1`, from the smaller
//!     `resources/example-references.json` file for fast iteration);
//!  2. vectorizing each payload with the same logic as the runtime
//!     extension (kept in sync via the duplicated `vectorize` here);
//!  3. doing an exhaustive k=5 search and counting how many of the
//!     5 nearest neighbors are labeled `fraud`.
//!
//! Output is a JSON array of `{id, expected_count}`.

use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

const D: usize = 14;

#[derive(Deserialize)]
struct ReferenceItem {
    vector: [f32; D],
    label: String,
}

#[derive(Deserialize)]
struct LastTx {
    timestamp: String,
    km_from_current: f32,
}

#[derive(Deserialize)]
struct TxPart {
    amount: f32,
    installments: u32,
    requested_at: String,
}

#[derive(Deserialize)]
struct CustomerPart {
    avg_amount: f32,
    tx_count_24h: u32,
    known_merchants: Vec<String>,
}

#[derive(Deserialize)]
struct MerchantPart {
    id: String,
    mcc: String,
    avg_amount: f32,
}

#[derive(Deserialize)]
struct TerminalPart {
    is_online: bool,
    card_present: bool,
    km_from_home: f32,
}

#[derive(Deserialize)]
struct Payload {
    id: String,
    transaction: TxPart,
    customer: CustomerPart,
    merchant: MerchantPart,
    terminal: TerminalPart,
    last_transaction: Option<LastTx>,
}

fn parse_iso(s: &str) -> (i32, u32, u32, u32, u32) {
    let b = s.as_bytes();
    let y = (b[0] - b'0') as i32 * 1000
        + (b[1] - b'0') as i32 * 100
        + (b[2] - b'0') as i32 * 10
        + (b[3] - b'0') as i32;
    let mo = (b[5] - b'0') as u32 * 10 + (b[6] - b'0') as u32;
    let d = (b[8] - b'0') as u32 * 10 + (b[9] - b'0') as u32;
    let h = (b[11] - b'0') as u32 * 10 + (b[12] - b'0') as u32;
    let mi = (b[14] - b'0') as u32 * 10 + (b[15] - b'0') as u32;
    (y, mo, d, h, mi)
}

fn day_of_week(y: u32, m: u32, d: u32) -> u32 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let ya = if m < 3 { y - 1 } else { y };
    let dow = (ya + ya / 4 - ya / 100 + ya / 400 + T[(m - 1) as usize] + d) % 7;
    (dow + 6) % 7
}

fn days_since_epoch(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as u32;
    let mm = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mm + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe as i64 - 719468
}

fn round4(x: f32) -> f32 { (x * 10000.0).round() * 0.0001 }
fn clamp01(v: f32) -> f32 { round4(v.clamp(0.0, 1.0)) }
fn mcc_risk_f(mcc: u32) -> f32 {
    match mcc {
        5411 => 0.15, 5812 => 0.30, 5912 => 0.20, 5944 => 0.45,
        7801 => 0.80, 7802 => 0.75, 7995 => 0.85, 4511 => 0.35,
        5311 => 0.25, 5999 => 0.50, _ => 0.50,
    }
}

fn vectorize(p: &Payload) -> [f32; 14] {
    let (ry, rmo, rd, rh, _) = parse_iso(&p.transaction.requested_at);
    let dow = day_of_week(ry as u32, rmo, rd) as u8;

    let (mins, km, has_last) = match &p.last_transaction {
        Some(lt) => {
            let (ly, lmo, ld, lh, lmi) = parse_iso(&lt.timestamp);
            let (ry2, rmo2, rd2, rh2, rmi2) = parse_iso(&p.transaction.requested_at);
            let d1 = days_since_epoch(ly, lmo, ld);
            let d2 = days_since_epoch(ry2, rmo2, rd2);
            let m1 = d1 * 1440 + lh as i64 * 60 + lmi as i64;
            let m2 = d2 * 1440 + rh2 as i64 * 60 + rmi2 as i64;
            ((m2 - m1).max(0) as u32, lt.km_from_current, true)
        }
        None => (0u32, 0.0f32, false),
    };

    let mcc: u32 = p.merchant.mcc.parse().unwrap_or(0);
    let unknown = !p.customer.known_merchants.iter().any(|m| m == &p.merchant.id);

    let mut v = [0.0f32; 14];
    v[0] = clamp01(p.transaction.amount / 10_000.0);
    v[1] = clamp01(p.transaction.installments as f32 / 12.0);
    let ratio = if p.customer.avg_amount > 0.0 {
        (p.transaction.amount / p.customer.avg_amount) / 10.0
    } else { 1.0 };
    v[2] = clamp01(ratio);
    v[3] = round4(rh as f32 / 23.0);
    v[4] = round4(dow as f32 / 6.0);
    if has_last {
        v[5] = clamp01(mins as f32 / 1440.0);
        v[6] = clamp01(km / 1000.0);
    } else {
        v[5] = -1.0;
        v[6] = -1.0;
    }
    v[7] = clamp01(p.terminal.km_from_home / 1000.0);
    v[8] = clamp01(p.customer.tx_count_24h as f32 / 20.0);
    v[9] = if p.terminal.is_online { 1.0 } else { 0.0 };
    v[10] = if p.terminal.card_present { 1.0 } else { 0.0 };
    v[11] = if unknown { 1.0 } else { 0.0 };
    v[12] = mcc_risk_f(mcc);
    v[13] = clamp01(p.merchant.avg_amount / 10_000.0);
    v
}

fn dist_sq(a: &[f32; 14], b: &[f32; 14]) -> f32 {
    let mut d = 0.0f32;
    for i in 0..14 { let x = a[i] - b[i]; d += x * x; }
    d
}

fn load_refs(path: &PathBuf) -> Vec<([f32; D], u8)> {
    let f = File::open(path).expect("open refs");
    let s = path.to_string_lossy();
    let mut buf = String::new();
    if s.ends_with(".gz") {
        GzDecoder::new(BufReader::new(f)).read_to_string(&mut buf).expect("gunzip");
    } else {
        BufReader::new(f).read_to_string(&mut buf).expect("read");
    }
    let items: Vec<ReferenceItem> = serde_json::from_str(&buf).expect("parse refs");
    items.into_iter().map(|i| (i.vector, if i.label == "fraud" { 1u8 } else { 0u8 })).collect()
}

fn main() {
    let refs_path = if std::env::var("RINHA_USE_EXAMPLE_REFS").ok().as_deref() == Some("1") {
        PathBuf::from("resources/example-references.json")
    } else {
        PathBuf::from("resources/references.json.gz")
    };
    let payloads_path = PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| "resources/example-payloads.json".into())
    );

    eprintln!("loading refs from {:?}", refs_path);
    let refs = load_refs(&refs_path);
    eprintln!("  {} reference vectors", refs.len());

    let txt = std::fs::read_to_string(&payloads_path).expect("read payloads");
    let payloads: Vec<Payload> = serde_json::from_str(&txt).expect("parse payloads");

    println!("[");
    for (i, p) in payloads.iter().enumerate() {
        let q = vectorize(p);
        // BinaryHeap is max-heap; we push (OrderedF32, label) and keep size 5.
        #[derive(PartialEq)]
        struct E(f32, u8);
        impl Eq for E {}
        impl PartialOrd for E { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { self.0.partial_cmp(&o.0) } }
        impl Ord for E { fn cmp(&self, o: &Self) -> std::cmp::Ordering { self.0.partial_cmp(&o.0).unwrap() } }
        let mut heap: BinaryHeap<E> = BinaryHeap::with_capacity(6);
        for (rv, rl) in refs.iter() {
            let d = dist_sq(&q, rv);
            if heap.len() < 5 {
                heap.push(E(d, *rl));
            } else if d < heap.peek().unwrap().0 {
                heap.pop();
                heap.push(E(d, *rl));
            }
        }
        let count: u32 = heap.iter().filter(|e| e.1 == 1).count() as u32;
        let comma = if i + 1 == payloads.len() { "" } else { "," };
        println!("  {{\"id\":\"{}\",\"expected_count\":{}}}{}", p.id, count, comma);
    }
    println!("]");
}
