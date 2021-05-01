use std::ffi::OsString;
use std::fmt::Display;
use std::fs::File;
use std::io::{stdout, BufReader, Error};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bigdecimal::{BigDecimal, FromPrimitive, One, ToPrimitive};
use blake2b_simd::Params;
use byteorder::{ByteOrder, NetworkEndian};
use chrono::{Duration, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use itertools::sorted;
use log::{debug, error, info, trace};
use num_bigint::{BigInt, Sign};
use rayon::prelude::*;
use rug::Rational;
use rusqlite::{named_params, Connection, OptionalExtension, NO_PARAMS};
use serde::{Deserialize, Serialize};
use serde_aux::prelude::deserialize_number_from_string;

use crate::nodeclient::leaderlog::deserialize::cbor_hex;
use crate::nodeclient::leaderlog::ledgerstate::calculate_ledger_state_sigma_d_and_extra_entropy;
use crate::nodeclient::leaderlog::libsodium::{sodium_crypto_vrf_proof_to_hash, sodium_crypto_vrf_prove};
use crate::nodeclient::math::{ln, normalize, round, taylor_exp_cmp, TaylorCmp};
use crate::nodeclient::{LedgerSet, PooltoolConfig};

mod deserialize;
pub mod ledgerstate;
mod libsodium;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LeaderLogError {
    status: String,
    error_message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ByronGenesis {
    start_time: i64,
    protocol_consts: ProtocolConsts,
    block_version_data: BlockVersionData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolConsts {
    k: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockVersionData {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    slot_duration: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShelleyGenesis {
    active_slots_coeff: f64,
    network_magic: u32,
    slot_length: i64,
    epoch_length: i64,
}

#[derive(Debug, Deserialize)]
struct VrfSkey {
    #[serde(rename(deserialize = "type"))]
    key_type: String,
    #[serde(deserialize_with = "cbor_hex")]
    #[serde(rename(deserialize = "cborHex"))]
    key: Vec<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LeaderLog {
    status: String,
    epoch: i64,
    epoch_nonce: String,
    epoch_slots: i64,
    epoch_slots_ideal: f64,
    max_performance: f64,
    pool_id: String,
    sigma: f64,
    active_stake: u64,
    total_active_stake: u64,
    d: f64,
    f: f64,
    assigned_slots: Vec<Slot>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Slot {
    no: i64,
    slot: i64,
    slot_in_epoch: i64,
    at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PooltoolSendSlots {
    api_key: String,
    pool_id: String,
    epoch: i64,
    slot_qty: i64,
    hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    prev_slots: Option<String>,
}

fn read_byron_genesis(byron_genesis: &Path) -> Result<ByronGenesis, Error> {
    let buf = BufReader::new(File::open(byron_genesis)?);
    Ok(serde_json::from_reader(buf)?)
}

fn read_shelley_genesis(shelley_genesis: &Path) -> Result<ShelleyGenesis, Error> {
    let buf = BufReader::new(File::open(shelley_genesis)?);
    Ok(serde_json::from_reader(buf)?)
}

fn read_vrf_skey(vrf_skey_path: &Path) -> Result<VrfSkey, Error> {
    let buf = BufReader::new(File::open(vrf_skey_path)?);
    Ok(serde_json::from_reader(buf)?)
}

fn get_tip_slot_number(db: &Connection) -> Result<i64, rusqlite::Error> {
    db.query_row("SELECT MAX(slot_number) FROM chain", NO_PARAMS, |row| row.get(0))
}

fn get_eta_v_before_slot(db: &Connection, slot_number: i64) -> Result<String, rusqlite::Error> {
    db.query_row("SELECT eta_v FROM chain WHERE orphaned = 0 AND slot_number < ?1 AND ?1 - slot_number < 120 ORDER BY slot_number DESC LIMIT 1", &[&slot_number], |row| {
        row.get(0)
    })
}

fn get_prev_hash_before_slot(db: &Connection, slot_number: i64) -> Result<String, rusqlite::Error> {
    db.query_row("SELECT prev_hash FROM chain WHERE orphaned = 0 AND slot_number < ?1 AND ?1 - slot_number < 120 ORDER BY slot_number DESC LIMIT 1", &[&slot_number], |row| {
        row.get(0)
    })
}

fn get_current_slots(db: &Connection, epoch: i64, pool_id: &str) -> Result<(i64, String), rusqlite::Error> {
    db.query_row_named(
        "SELECT slot_qty, hash FROM slots WHERE epoch = :epoch AND pool_id = :pool_id LIMIT 1",
        named_params! {
                ":epoch": epoch,
                ":pool_id": pool_id,
        },
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

fn get_prev_slots(db: &Connection, epoch: i64, pool_id: &str) -> Result<Option<String>, rusqlite::Error> {
    db.query_row_named(
        "SELECT slots FROM slots WHERE epoch = :epoch AND pool_id = :pool_id LIMIT 1",
        named_params! {
                ":epoch": epoch,
                ":pool_id": pool_id,
        },
        |row| row.get(0),
    )
    .optional()
}

fn get_shelley_transition_epoch(network_magic: u32) -> i64 {
    match network_magic {
        764824073 => {
            // mainnet
            208
        }
        141 => {
            // guild
            1
        }
        _ => {
            // testnet
            74
        }
    }
}

fn get_first_slot_of_epoch(byron: &ByronGenesis, shelley: &ShelleyGenesis, current_slot: i64) -> (i64, i64) {
    let shelley_transition_epoch = get_shelley_transition_epoch(shelley.network_magic);
    let byron_epoch_length = 10 * byron.protocol_consts.k;
    let byron_slots = byron_epoch_length * shelley_transition_epoch;
    let shelley_slots = current_slot - byron_slots;
    let shelley_slot_in_epoch = shelley_slots % shelley.epoch_length;
    let first_slot_of_epoch = current_slot - shelley_slot_in_epoch;
    let epoch = (shelley_slots / shelley.epoch_length) + shelley_transition_epoch;

    (epoch, first_slot_of_epoch)
}

fn slot_to_naivedatetime(byron: &ByronGenesis, shelley: &ShelleyGenesis, slot: i64) -> NaiveDateTime {
    let shelley_transition_epoch = get_shelley_transition_epoch(shelley.network_magic);
    let network_start_time = NaiveDateTime::from_timestamp(byron.start_time, 0);
    let byron_epoch_length = 10 * byron.protocol_consts.k;
    let byron_slots = byron_epoch_length * shelley_transition_epoch;
    let shelley_slots = slot - byron_slots;

    let byron_secs = (byron.block_version_data.slot_duration * byron_slots) / 1000;
    let shelley_secs = shelley_slots * shelley.slot_length;

    network_start_time + Duration::seconds(byron_secs) + Duration::seconds(shelley_secs)
}

fn slot_to_timestamp(byron: &ByronGenesis, shelley: &ShelleyGenesis, slot: i64, tz: &Tz) -> String {
    let slot_time = slot_to_naivedatetime(byron, shelley, slot);
    tz.from_utc_datetime(&slot_time).to_rfc3339()
}

pub fn is_overlay_slot(first_slot_of_epoch: &i64, current_slot: &i64, d: &Rational) -> bool {
    trace!("d: {:?}", &d);
    let diff_slot = Rational::from((current_slot - first_slot_of_epoch).abs());
    trace!("diff_slot: {:?}", &diff_slot);
    let diff_slot_inc: Rational = Rational::from(&diff_slot + 1);
    trace!("diff_slot_inc: {:?}", &diff_slot_inc);
    let left = (d * diff_slot).ceil();
    trace!("left: {:?}", &left);
    let right = (d * diff_slot_inc).ceil();
    trace!("right: {:?}", &right);
    left < right
}

//
// The universal constant nonce. The blake2b hash of the 8 byte long value of 1
// 12dd0a6a7d0e222a97926da03adb5a7768d31cc7c5c2bd6828e14a7d25fa3a60
// Sometimes called seedL in the haskell code
//
const UC_NONCE: [u8; 32] = [
    0x12, 0xdd, 0x0a, 0x6a, 0x7d, 0x0e, 0x22, 0x2a, 0x97, 0x92, 0x6d, 0xa0, 0x3a, 0xdb, 0x5a, 0x77, 0x68, 0xd3, 0x1c,
    0xc7, 0xc5, 0xc2, 0xbd, 0x68, 0x28, 0xe1, 0x4a, 0x7d, 0x25, 0xfa, 0x3a, 0x60,
];

fn mk_seed(slot: i64, eta0: &[u8]) -> Vec<u8> {
    trace!("mk_seed() start slot {}", slot);
    let mut concat = [0u8; 8 + 32];
    NetworkEndian::write_i64(&mut concat, slot);
    concat[8..].copy_from_slice(eta0);
    trace!("concat: {}", hex::encode(&concat));

    let slot_to_seed = Params::new()
        .hash_length(32)
        .to_state()
        .update(&concat)
        .finalize()
        .as_bytes()
        .to_owned();

    UC_NONCE
        .iter()
        .enumerate()
        .map(|(i, byte)| byte ^ slot_to_seed[i])
        .collect()
}

fn vrf_eval_certified(seed: &[u8], pool_vrf_skey: &[u8]) -> Result<BigInt, String> {
    let certified_proof: Vec<u8> = sodium_crypto_vrf_prove(pool_vrf_skey, seed)?;
    let certified_proof_hash: Vec<u8> = sodium_crypto_vrf_proof_to_hash(&*certified_proof)?;
    Ok(BigInt::from_bytes_be(Sign::Plus, &*certified_proof_hash))
}

// Determine if our pool is a slot leader for this given slot
// @param slot The slot to check
// @param sigma The controlled stake proportion for the pool
// @param eta0 The epoch nonce value
// @param pool_vrf_skey The vrf signing key for the pool
// @param cert_nat_max The value 2^512
// @param c 1-activeSlotsCoeff - usually 0.95
fn is_slot_leader(
    slot: i64,
    sigma: &BigDecimal,
    eta0: &[u8],
    pool_vrf_skey: &[u8],
    cert_nat_max: &BigDecimal,
    c: &BigDecimal,
) -> Result<bool, String> {
    trace!("is_slot_leader: {}", slot);
    let seed: Vec<u8> = mk_seed(slot, eta0);
    trace!("seed: {}", hex::encode(&seed));
    let cert_nat: BigInt = vrf_eval_certified(&*seed, pool_vrf_skey)?;
    trace!("cert_nat: {}", &cert_nat);
    let denominator = cert_nat_max - BigDecimal::from(cert_nat);
    let recip_q: BigDecimal = normalize(cert_nat_max / denominator);
    trace!("recip_q: {}", &recip_q);
    trace!("c: {}", c);
    let x: BigDecimal = round(-c * sigma);
    trace!("x: {}", &x);

    match taylor_exp_cmp(3, &recip_q, &x) {
        TaylorCmp::Above => Ok(false),
        TaylorCmp::Below => Ok(true),
        TaylorCmp::MaxReached => Ok(false),
    }
}

#[rustfmt::skip] // very deep nesting needs a refactor
#[allow(clippy::too_many_arguments)]
pub(crate) fn calculate_leader_logs(
    db_path: &Path,
    byron_genesis: &Path,
    shelley_genesis: &Path,
    pool_stake: &Option<u64>,
    active_stake: &Option<u64>,
    extra_entropy: &Option<String>,
    ledger_state: &str,
    ledger_set: &LedgerSet,
    pool_id: &str,
    pool_vrf_skey_path: &Path,
    timezone: &str,
    is_just_nonce: bool,
) {
    let tz: Tz = match timezone.parse::<Tz>() {
        Err(_) => {
            handle_error("timezone parse error!");
            return;
        }
        Ok(zone) => zone,
    };

    if !db_path.exists() {
        handle_error(format!("Invalid Path: --db {}", db_path.to_string_lossy()));
        return;
    }

    if !byron_genesis.exists() {
        handle_error(format!(
            "Invalid Path: --byron-genesis {}",
            byron_genesis.to_string_lossy()
        ));
        return;
    }

    if !shelley_genesis.exists() {
        handle_error(format!(
            "Invalid Path: --shelley-genesis {}",
            shelley_genesis.to_string_lossy()
        ));
        return;
    }

    let mut is_ledger_api = false;
    if !is_just_nonce && !pool_vrf_skey_path.exists() {
        handle_error(format!(
            "Invalid Path: --pool_vrf_skey {}",
            pool_vrf_skey_path.to_string_lossy()
        ));
        return;
    }

    if ledger_state.starts_with("http://") || ledger_state.starts_with("https://") {
        is_ledger_api = true;
        if pool_stake.is_some() ^ active_stake.is_some() {
            handle_error("Both --pool-stake and --active-stake must be defined");
            return;
        }
    } else if !PathBuf::from(OsString::from_str(ledger_state).unwrap()).exists() {
        handle_error(format!(
            "Invalid Path: --ledger-state {}",
            ledger_state
        ));
        return;
    }

    let db = Connection::open(db_path).unwrap();

    match read_byron_genesis(byron_genesis) {
        Ok(byron) => {
            debug!("{:?}", byron);
            match read_shelley_genesis(shelley_genesis) {
                Ok(shelley) => {
                    debug!("{:?}", shelley);

                    let tip_slot_number = get_tip_slot_number(&db).unwrap();
                    debug!("tip_slot_number: {}", tip_slot_number);

                    // Make sure we're fully sync'd
                    let tip_time = slot_to_naivedatetime(&byron, &shelley, tip_slot_number).timestamp();
                    let system_time = Utc::now().timestamp();
                    if system_time - tip_time > 900 {
                        handle_error(format!(
                            "db not fully synced! system_time: {}, tip_time: {}",
                            system_time, tip_time
                        ));
                        return;
                    }

                    // pretend we're on a different slot number if we want to calculate past or future epochs.
                    let additional_slots: i64 = match ledger_set {
                        LedgerSet::Mark => shelley.epoch_length,
                        LedgerSet::Set => 0,
                        LedgerSet::Go => -shelley.epoch_length,
                    };
                    let (epoch, first_slot_of_epoch) =
                        get_first_slot_of_epoch(&byron, &shelley, tip_slot_number + additional_slots);
                    debug!("epoch: {}", epoch);
                    let first_slot_of_prev_epoch = first_slot_of_epoch - shelley.epoch_length;
                    debug!("first_slot_of_epoch: {}", first_slot_of_epoch);
                    debug!("first_slot_of_prev_epoch: {}", first_slot_of_prev_epoch);
                    let stability_window: i64 =
                        ((3 * byron.protocol_consts.k) as f64 / shelley.active_slots_coeff).ceil() as i64;
                    let stability_window_start = first_slot_of_epoch - stability_window;
                    debug!("stability_window: {}", stability_window);
                    debug!("stability_window_start: {}", stability_window_start);

                    match get_eta_v_before_slot(&db, stability_window_start) {
                        Ok(nc) => {
                            debug!("nc: {}", nc);
                            match calculate_ledger_state_sigma_d_and_extra_entropy(pool_stake, active_stake, extra_entropy, ledger_state, ledger_set, pool_id, epoch, is_ledger_api, is_just_nonce) {
                                Ok(ledger_info) => {
                                    match get_prev_hash_before_slot(&db, first_slot_of_prev_epoch) {
                                        Ok(nh) => {
                                            debug!("nh: {}", nh);
                                            let mut nc_nh = String::new();
                                            nc_nh.push_str(&*nc);
                                            nc_nh.push_str(&*nh);
                                            let epoch_nonce = Params::new()
                                                .hash_length(32)
                                                .to_state()
                                                .update(&*hex::decode(nc_nh).unwrap())
                                                .finalize()
                                                .as_bytes()
                                                .to_owned();

                                            let epoch_nonce = match &ledger_info.extra_entropy {
                                                None => { epoch_nonce }
                                                Some(entropy) => {
                                                    let mut nonce_entropy = String::new();
                                                    nonce_entropy.push_str(&*hex::encode(&epoch_nonce));
                                                    nonce_entropy.push_str(&*entropy);
                                                    Params::new()
                                                        .hash_length(32)
                                                        .to_state()
                                                        .update(&*hex::decode(nonce_entropy).unwrap())
                                                        .finalize()
                                                        .as_bytes()
                                                        .to_owned()
                                                }
                                            };

                                            debug!("epoch_nonce: {}", hex::encode(&epoch_nonce));
                                            if is_just_nonce {
                                                println!("{}", hex::encode(&epoch_nonce));
                                                return;
                                            }

                                            match read_vrf_skey(pool_vrf_skey_path) {
                                                Ok(pool_vrf_skey) => {
                                                    if pool_vrf_skey.key_type != "VrfSigningKey_PraosVRF" {
                                                        handle_error("Pool VRF Skey must be of type: VrfSigningKey_PraosVRF");
                                                        return;
                                                    }

                                                    let sigma = normalize(
                                                        BigDecimal::from(ledger_info.sigma.0)
                                                            / BigDecimal::from(ledger_info.sigma.1),
                                                    );
                                                    debug!("sigma: {:?}", &sigma);
                                                    debug!("decentralization_param: {:?}", &ledger_info.decentralization);
                                                    debug!("extra_entropy: {:?}", &ledger_info.extra_entropy);

                                                    let d: f64 =
                                                        (ledger_info.decentralization.to_f64() * 100.0).round() / 100.0;
                                                    let epoch_slots_ideal = (sigma.to_f64().unwrap()
                                                        * (shelley.epoch_length.to_f64().unwrap()
                                                        * shelley.active_slots_coeff)
                                                        * (1.0 - d)
                                                        * 100.0)
                                                        .round()
                                                        / 100.0;
                                                    let mut leader_log = LeaderLog {
                                                        status: "ok".to_string(),
                                                        epoch,
                                                        epoch_nonce: hex::encode(&epoch_nonce),
                                                        epoch_slots: 0,
                                                        epoch_slots_ideal,
                                                        max_performance: 0.0,
                                                        pool_id: pool_id.to_string(),
                                                        sigma: sigma.to_f64().unwrap(),
                                                        active_stake: ledger_info.sigma.0,
                                                        total_active_stake: ledger_info.sigma.1,
                                                        d,
                                                        f: shelley.active_slots_coeff,
                                                        assigned_slots: vec![],
                                                    };

                                                    let cert_nat_max: BigDecimal = BigDecimal::from_str("13407807929942597099574024998205846127479365820592393377723561443721764030073546976801874298166903427690031858186486050853753882811946569946433649006084096").unwrap(); // 2^512
                                                    let c: BigDecimal = ln(&(BigDecimal::one()
                                                        - BigDecimal::from_f64(shelley.active_slots_coeff).unwrap()));

                                                    // Calculate all of our assigned slots in the epoch (in parallel)
                                                    let assigned_slots = (0..shelley.epoch_length)
                                                        .par_bridge() // <--- use rayon parallel bridge
                                                        .map(|slot_in_epoch| first_slot_of_epoch + slot_in_epoch)
                                                        .filter(|epoch_slot| !is_overlay_slot(&first_slot_of_epoch, &epoch_slot, &ledger_info.decentralization))
                                                        .filter_map(|leader_slot| {
                                                            match is_slot_leader(leader_slot, &sigma, &epoch_nonce, &pool_vrf_skey.key, &cert_nat_max, &c) {
                                                                Ok(true) => Some(leader_slot),
                                                                Ok(false) => None,
                                                                Err(msg) => {
                                                                    handle_error(msg);
                                                                    None
                                                                }
                                                            }
                                                        }).collect::<Vec<_>>();

                                                    // Update leader log with all assigned slots (sort first)
                                                    for (i, slot) in sorted(assigned_slots.iter()).enumerate() {
                                                        let no = (i + 1) as i64;
                                                        let slot = Slot {
                                                            no,
                                                            slot: *slot,
                                                            slot_in_epoch: slot - first_slot_of_epoch,
                                                            at: slot_to_timestamp(&byron, &shelley, *slot, &tz),
                                                        };

                                                        debug!("Found assigned slot: {:?}", &slot);
                                                        leader_log.assigned_slots.push(slot);
                                                        leader_log.epoch_slots = no;
                                                    };

                                                    // Calculate expected performance
                                                    leader_log.max_performance = (leader_log.epoch_slots as f64 / epoch_slots_ideal * 10000.0).round() / 100.0;

                                                    // Save slots to database so we can send to pooltool later
                                                    match db.prepare("INSERT INTO slots (epoch,pool_id,slot_qty,slots,hash) VALUES (:epoch,:pool_id,:slot_qty,:slots,:hash) ON CONFLICT (epoch,pool_id) DO UPDATE SET slot_qty=excluded.slot_qty, slots=excluded.slots, hash=excluded.hash") {
                                                        Ok(mut insert_slots_statement) => {
                                                            let mut slots = String::new();
                                                            slots.push('[');
                                                            for (i, assigned_slot) in leader_log.assigned_slots.iter().enumerate() {
                                                                if i > 0 {
                                                                    slots.push(',');
                                                                }
                                                                slots.push_str(&*assigned_slot.slot.to_string())
                                                            }
                                                            slots.push(']');

                                                            let hash = hex::encode(Params::new().hash_length(32).to_state().update(slots.as_ref()).finalize().as_bytes().to_vec());

                                                            match insert_slots_statement.execute_named(
                                                                named_params! {
                                                                    ":epoch" : epoch,
                                                                    ":pool_id" : pool_id,
                                                                    ":slot_qty" : assigned_slots.len() as i64,
                                                                    ":slots" : slots,
                                                                    ":hash" : hash
                                                                }
                                                            ) {
                                                                Ok(_) => {
                                                                    match serde_json::to_string_pretty(&leader_log) {
                                                                        Ok(leader_log_json) => {
                                                                            println!("{}", leader_log_json);
                                                                        }
                                                                        Err(error) => { handle_error(error) }
                                                                    }
                                                                }
                                                                Err(error) => { handle_error(error) }
                                                            }
                                                        }
                                                        Err(error) => { handle_error(error) }
                                                    }
                                                }
                                                Err(error) => { handle_error(error) }
                                            }
                                        }
                                        Err(error) => { handle_error(error) }
                                    }
                                }
                                Err(error) => { handle_error(error) }
                            }
                        }
                        Err(error) => { handle_error(error) }
                    }
                }
                Err(error) => { handle_error(error) }
            }
        }
        Err(error) => { handle_error(error) }
    }

    if let Err(error) = db.close() {
        handle_error(format!("db close error: {}", error.1));
    }
}

pub(crate) fn status(db_path: &Path, byron_genesis: &Path, shelley_genesis: &Path) {
    if !db_path.exists() {
        handle_error("database not found!");
        return;
    }
    let db = Connection::open(db_path).unwrap();

    match read_byron_genesis(byron_genesis) {
        Ok(byron) => {
            debug!("{:?}", byron);
            match read_shelley_genesis(shelley_genesis) {
                Ok(shelley) => {
                    debug!("{:?}", shelley);
                    match get_tip_slot_number(&db) {
                        Ok(tip_slot_number) => {
                            debug!("tip_slot_number: {}", tip_slot_number);
                            let tip_time = slot_to_naivedatetime(&byron, &shelley, tip_slot_number).timestamp();
                            let system_time = Utc::now().timestamp();
                            if system_time - tip_time < 120 {
                                print_status_synced();
                            } else {
                                handle_error("db not fully synced!")
                            }
                        }
                        Err(error) => handle_error(error),
                    }
                }
                Err(error) => handle_error(error),
            }
        }
        Err(error) => handle_error(error),
    }

    if let Err(error) = db.close() {
        handle_error(format!("db close error: {}", error.1));
    }
}

pub(crate) fn send_slots(
    db_path: &Path,
    byron_genesis: &Path,
    shelley_genesis: &Path,
    pooltool_config: PooltoolConfig,
) {
    if !db_path.exists() {
        handle_error("database not found!");
        return;
    }
    let db = Connection::open(db_path).unwrap();

    match read_byron_genesis(byron_genesis) {
        Ok(byron) => {
            debug!("{:?}", byron);
            match read_shelley_genesis(shelley_genesis) {
                Ok(shelley) => {
                    debug!("{:?}", shelley);
                    match get_tip_slot_number(&db) {
                        Ok(tip_slot_number) => {
                            debug!("tip_slot_number: {}", tip_slot_number);
                            let tip_time = slot_to_naivedatetime(&byron, &shelley, tip_slot_number).timestamp();
                            let system_time = Utc::now().timestamp();
                            if system_time - tip_time < 120 {
                                let (epoch, _) = get_first_slot_of_epoch(&byron, &shelley, tip_slot_number);
                                debug!("epoch: {}", epoch);
                                for pool in pooltool_config.pools.iter() {
                                    match get_current_slots(&db, epoch, &pool.pool_id) {
                                        Ok((slot_qty, hash)) => {
                                            debug!("slot_qty: {}", slot_qty);
                                            debug!("hash: {}", &hash);
                                            match get_prev_slots(&db, epoch - 1, &pool.pool_id) {
                                                Ok(prev_slots) => {
                                                    let request = serde_json::ser::to_string(&PooltoolSendSlots {
                                                        api_key: pooltool_config.api_key.clone(),
                                                        pool_id: pool.pool_id.clone(),
                                                        epoch,
                                                        slot_qty,
                                                        hash,
                                                        prev_slots,
                                                    })
                                                    .unwrap();
                                                    info!("Sending: {}", &request);
                                                    match reqwest::blocking::Client::builder().build() {
                                                        Ok(client) => {
                                                            let pooltool_result = client
                                                                .post("https://api.pooltool.io/v0/sendslots")
                                                                .body(request)
                                                                .send();

                                                            match pooltool_result {
                                                                Ok(response) => match response.text() {
                                                                    Ok(text) => {
                                                                        info!("Pooltool Response: {}", text);
                                                                    }
                                                                    Err(error) => {
                                                                        error!("PoolTool error: {}", error);
                                                                    }
                                                                },
                                                                Err(error) => {
                                                                    error!("PoolTool error: {}", error);
                                                                }
                                                            }
                                                        }
                                                        Err(err) => {
                                                            error!("Could not set up the reqwest client!: {}", err)
                                                        }
                                                    }
                                                }
                                                Err(error) => {
                                                    error!("Db Error: {}", error)
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            error!("Cannot find db record for {},{}: {}", epoch, &pool.pool_id, error)
                                        }
                                    }
                                }
                            } else {
                                handle_error("db not fully synced!")
                            }
                        }
                        Err(error) => handle_error(error),
                    }
                }
                Err(error) => handle_error(error),
            }
        }
        Err(error) => handle_error(error),
    }

    if let Err(error) = db.close() {
        handle_error(format!("db close error: {}", error.1));
    }
}

fn print_status_synced() {
    println!(
        "{{\n\
            \x20\"status\": \"ok\"\n\
            }}"
    );
}

pub fn handle_error<T: Display>(error_message: T) {
    serde_json::ser::to_writer_pretty(
        &mut stdout(),
        &LeaderLogError {
            status: "error".to_string(),
            error_message: format!("{}", error_message),
        },
    )
    .unwrap();
}
