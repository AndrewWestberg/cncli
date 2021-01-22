use std::str::FromStr;

use bigdecimal::BigDecimal;
use rug::{Float, Rational};
use rug::float::Round;
use rug::ops::{MulAssignRound, SubFrom};

use cncli::nodeclient::ping;
use nodeclient::leaderlog::is_overlay_slot;

use super::*;
use std::ops::Div;

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
fn test_ln() {
    let mut c: Float = Float::with_val(120, 0.05_f64);
    c.sub_from(1);
    c.ln_round(Round::Down);
    println!("c: {}", &c.to_string_radix(10, None));
    println!("ln(1-f) = ln (0.95) = -0.0512932943875505334261962382072846");

    let f = BigDecimal::from_str("0.05").unwrap().with_prec(10).with_scale(34);
    let other = BigDecimal::from(27u8).with_prec(10).with_scale(34);
    println!("f: {}", f.div(other).with_scale(35).round(34));
}