use std::str::FromStr;

use bigdecimal::{BigDecimal, One, Zero};
use rug::float::Round;
use rug::ops::MulAssignRound;
use rug::{Float, Rational};

use cncli::nodeclient::math::{ceiling, exp, find_e, ln, round, split_ln};
use cncli::nodeclient::ping;
use nodeclient::leaderlog::is_overlay_slot;
use nodeclient::math::ipow;

use super::*;

#[test]
fn test_is_overlay_slot() {
    pretty_env_logger::init_timed();

    let first_slot_of_epoch = 15724800_i64;
    let mut current_slot = 16128499_i64;
    // let d = Float::with_val(120, 0.32);
    let mut d = Float::with_val(24, Float::parse("0.32").unwrap());
    d.mul_assign_round(100, Round::Nearest);
    let r: Rational = Rational::from((d.to_integer().unwrap(), 100));

    assert_eq!(is_overlay_slot(&first_slot_of_epoch, &current_slot, &r), false);

    // AD test
    current_slot = 15920150_i64;
    assert_eq!(is_overlay_slot(&first_slot_of_epoch, &current_slot, &r), true);
}

#[test]
fn test_ping() {
    let host = "north-america.relays-new.cardano-testnet.iohkdev.io".to_string();
    let port = 3001;
    let network_magic = 1097911063;
    let mut stdout: Vec<u8> = Vec::new();

    ping::ping(&mut stdout, &host, port, network_magic);

    assert_eq!(&std::str::from_utf8(&stdout).unwrap()[..102], "{\n  \"status\": \"ok\",\n  \"host\": \"north-america.relays-new.cardano-testnet.iohkdev.io\",\n  \"port\": 3001,\n ");
}

#[test]
fn test_ping_failure_address() {
    let host = "murrika.relays-new.cardano-testnet.iohkdev.io".to_string();
    let port = 3001;
    let network_magic = 1097911063;
    let mut stdout: Vec<u8> = Vec::new();

    ping::ping(&mut stdout, &host, port, network_magic);

    #[cfg(target_os = "macos")]
    assert_eq!(&std::str::from_utf8(&stdout).unwrap()[..], "{\n  \"status\": \"error\",\n  \"host\": \"murrika.relays-new.cardano-testnet.iohkdev.io\",\n  \"port\": 3001,\n  \"errorMessage\": \"failed to lookup address information: nodename nor servname provided, or not known\"\n}");

    #[cfg(target_os = "linux")]
    assert_eq!(&std::str::from_utf8(&stdout).unwrap()[..], "{\n  \"status\": \"error\",\n  \"host\": \"murrika.relays-new.cardano-testnet.iohkdev.io\",\n  \"port\": 3001,\n  \"errorMessage\": \"failed to lookup address information: Name or service not known\"\n}");
}

#[test]
fn test_ping_failure_bad_port() {
    let host = "north-america.relays-new.cardano-testnet.iohkdev.io".to_string();
    let port = 3992;
    let network_magic = 1097911063;
    let mut stdout: Vec<u8> = Vec::new();

    ping::ping(&mut stdout, &host, port, network_magic);

    assert_eq!(&std::str::from_utf8(&stdout).unwrap()[..], "{\n  \"status\": \"error\",\n  \"host\": \"north-america.relays-new.cardano-testnet.iohkdev.io\",\n  \"port\": 3992,\n  \"errorMessage\": \"connection timed out\"\n}");
}

#[test]
fn test_ping_failure_bad_magic() {
    let host = "north-america.relays-new.cardano-testnet.iohkdev.io".to_string();
    let port = 3001;
    let network_magic = 111111;
    let mut stdout: Vec<u8> = Vec::new();

    ping::ping(&mut stdout, &host, port, network_magic);

    assert_eq!(&std::str::from_utf8(&stdout).unwrap()[..], "{\n  \"status\": \"error\",\n  \"host\": \"north-america.relays-new.cardano-testnet.iohkdev.io\",\n  \"port\": 3001,\n  \"errorMessage\": \"version data mismatch: NodeToNodeVersionData {networkMagic = NetworkMagic {unNetworkMagic = 1097911063}, diffusionMode = InitiatorAndResponderDiffusionMode} /= NodeToNodeVersionData {networkMagic = NetworkMagic {unNetworkMagic = 111111}, diffusionMode = InitiatorAndResponderDiffusionMode}\"\n}");
}

#[test]
fn test_eps() {
    let eps = BigDecimal::from_str("1.E-24").unwrap();
    // println!("1/10^24 = {}", eps);
    assert_eq!(eps.to_string(), "0.000000000000000000000001");
}

#[test]
fn test_ceiling() {
    let x = BigDecimal::from_str("1234.00000").unwrap();
    let ceil_x = ceiling(&x);
    assert_eq!(ceil_x, BigDecimal::from(1234));

    let y = BigDecimal::from_str("1234.0000000000123").unwrap();
    let ceil_y = ceiling(&y);
    assert_eq!(ceil_y, BigDecimal::from(1235))
}

#[test]
fn test_exp() {
    let x = BigDecimal::zero();
    let exp_x = exp(&x);
    assert_eq!(exp_x, BigDecimal::one());

    let x = BigDecimal::one();
    let exp_x = exp(&x);
    assert_eq!(exp_x.to_string(), "2.7182818284590452353602874043083282");

    let x = BigDecimal::from_str("-54.268914").unwrap();
    let exp_x = exp(&x);
    assert_eq!(exp_x.to_string(), "0.0000000000000000000000026996664594");
}

#[test]
fn test_find_e() {
    let exp1 = exp(&BigDecimal::one());
    let x = BigDecimal::from_str("8.5").unwrap();
    let n = find_e(&exp1, &x);
    println!("find_e({}) = {}", &x, n);
    assert_eq!(n, 2);
}

#[test]
fn test_split_ln() {
    let exp1 = exp(&BigDecimal::one());
    let x = BigDecimal::from_str("2.9").unwrap();
    let (n, xp) = split_ln(&exp1, &x);
    println!("n: {}, xp: {}", n, xp);
}

#[test]
fn test_ln() {
    let x = BigDecimal::one();
    let ln_x = ln(&x);
    assert_eq!(ln_x.to_string(), "0.0000000000000000000000000000000000");

    let x = BigDecimal::from_str("0.95").unwrap();
    let ln_x = ln(&x);
    assert_eq!(ln_x.to_string(), "-0.0512932943875505334261962382072846");
    println!("ln(1-f) = ln (0.95) = {}", ln_x);
}

#[test]
fn test_infinite_range_stuff() {
    let mut range = 1..;

    let mut i = 0;

    while i < 1024 {
        let an = range.by_ref().take(1).next().unwrap();
        let bn = an * an;
        println!("an: {}, bn: {}, range: {:?}", an, bn, &range);
        i += 1;
    }

    // let x:&[i32] = &(1..1025).collect()[..];
    // let y: &[i32] = &(1..1025).map(|m| m * m).collect()[..];
    //
    // println!("x: {:?}", x);
    // println!("y: {:?}", y);
}

#[test]
fn test_pow() {
    let mut x = BigDecimal::from_str("0.2587").unwrap();
    let mut y = ipow(&x, 5);
    println!("{}^5 = {}", x, y);

    x = BigDecimal::from_str("-17.2589").unwrap();
    y = ipow(&x, 5);
    println!("{}^5 = {}", x, y);

    x = BigDecimal::from(2);
    y = ipow(&x, 512);
    println!("{}^512 = {}", x, y);
}

#[test]
fn test_leaderlog_math() {
    let sigma = BigDecimal::from_str("0.0077949348290607914969808129687391").unwrap();
    let c = BigDecimal::from_str("-0.0512932943875505334261962382072846").unwrap();
    //let x = round(&(-c * sigma), 34);
    let x = round(-c * sigma);
    println!("x: {}", x);
    assert_eq!(x.to_string(), "0.0003998278869187860731522824872380")
}

// #[test]
// fn test_ledger_state_1_26_0() {
//     // Calculate values from json
//     let ledger_state: &PathBuf = &PathBuf::from(OsString::from_str("/tmp/ledger-state-guild.json").unwrap());
//     let ledger: Ledger =
//         match serde_json::from_reader::<BufReader<File>, Ledger3>(BufReader::new(File::open(ledger_state).unwrap())) {
//             Ok(ledger3) => ledger3.state_before,
//             Err(error) => {
//                 panic!("Failed to parse ledger state: {:?}", error);
//             }
//         };
//
//     println!("ledger: {:?}", ledger);
// }
