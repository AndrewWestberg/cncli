use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind};
use std::path::PathBuf;
use std::str::FromStr;

use log::debug;
use rug::Rational;
use serde::Deserialize;

use crate::nodeclient::leaderlog::deserialize::{rational, rational_optional};
use crate::nodeclient::LedgerSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger2 {
    nes_es: Ledger,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    es_prev_pp: ProtocolParams,
    es_pp: ProtocolParams,
    es_l_state: EsLState,
    es_snapshots: EsSnapshots,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolParams {
    #[serde(deserialize_with = "rational")]
    decentralisation_param: Rational,
}

#[derive(Debug, Deserialize)]
struct EsLState {
    #[serde(rename(deserialize = "_utxoState"))]
    utxo_state: UtxoState,
}

#[derive(Debug, Deserialize)]
struct UtxoState {
    #[serde(rename(deserialize = "_ppups"))]
    ppups: Ppups,
}

#[derive(Debug, Deserialize)]
struct Ppups {
    proposals: Proposals,
}

#[derive(Debug, Deserialize)]
struct Proposals {
    #[serde(flatten)]
    proposal: HashMap<String, Proposal>,
}

#[derive(Debug, Deserialize)]
struct Proposal {
    #[serde(rename(deserialize = "_d"))]
    #[serde(deserialize_with = "rational_optional")]
    decentralisation_param: Option<Rational>,
}

#[derive(Debug, Deserialize)]
struct EsSnapshots {
    #[serde(rename(deserialize = "_pstakeMark"))]
    stake_mark: StakeGroup,
    #[serde(rename(deserialize = "_pstakeSet"))]
    stake_set: StakeGroup,
    #[serde(rename(deserialize = "_pstakeGo"))]
    stake_go: StakeGroup,
}

#[derive(Debug, Deserialize)]
struct StakeGroup {
    #[serde(rename(deserialize = "_stake"))]
    stake: Vec<Vec<Stake>>,
    #[serde(rename(deserialize = "_delegations"))]
    delegations: Vec<Vec<Delegation>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Stake {
    StakeKey(Key),
    Lovelace(u64),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Delegation {
    StakeKey(Key),
    PoolId(String),
}

#[derive(Debug, Deserialize)]
struct Key {
    #[serde(rename(deserialize = "key hash"))]
    key: String,
}

#[derive(Debug, Deserialize)]
struct LedgerApiResponse {
    #[serde(rename(deserialize = "d"))]
    #[serde(deserialize_with = "rational")]
    decentralisation_param: Rational,
    active_stake: u64,
    total_staked: u64,
    // {"d":"0.16","total_staked":"22369166376492895","active_stake":"8193623134725","sigma":0.00036629094695882095,"nonce":"6de5370ca56cd7ff8cbca5ddf216f345417708b2a12b8b8c61ac73c3733cce57"}
}

fn calculate_sigma(stake_group: StakeGroup, pool_id: &str) -> (u64, u64) {
    let stake_keys: Vec<String> = stake_group
        .delegations
        .into_iter()
        .filter_map(|delegation| {
            if delegation.len() != 2 {
                return None;
            }
            let mut out_pool_id: String = "".to_string();
            let mut stake_key: String = "".to_string();
            for item in delegation.into_iter() {
                match item {
                    Delegation::StakeKey(key) => stake_key = key.key,
                    Delegation::PoolId(poolid) => out_pool_id = poolid,
                }
            }

            if out_pool_id != *pool_id {
                None
            } else {
                debug!("Found delegation key: {:?}", &stake_key);
                Some(stake_key)
            }
        })
        .collect();

    let mut denominator = 0u64;
    let numerator: u64 = stake_group
        .stake
        .into_iter()
        .filter_map(|stake| {
            if stake.len() != 2 {
                return None;
            }
            let mut lovelace = 0u64;
            let mut key: String = "".to_string();
            for item in stake.into_iter() {
                match item {
                    Stake::StakeKey(stake_key) => key = stake_key.key,
                    Stake::Lovelace(amount) => lovelace = amount,
                }
            }
            denominator += lovelace;

            if stake_keys.iter().any(|delegated_key| *delegated_key == key) {
                debug!("Found delegated amount: {}", lovelace);
                Some(lovelace)
            } else {
                None
            }
        })
        .sum();
    debug!("activeStake: {}", numerator);
    debug!("totalActiveStake: {}", denominator);
    (numerator, denominator)
}

pub(super) fn calculate_ledger_state_sigma_and_d(
    ledger_state: &str,
    ledger_set: &LedgerSet,
    pool_id: &str,
    epoch: i64,
    is_ledger_api: bool,
) -> Result<((u64, u64), Rational), Error> {
    if is_ledger_api {
        match reqwest::blocking::Client::builder().build() {
            Ok(client) => {
                let mut url = ledger_state.to_owned();
                url.push('/');
                url.push_str(pool_id);
                url.push('/');
                url.push_str(&*epoch.to_string());
                let api_result = client.get(&url).send();

                match api_result {
                    Ok(response) => match response.text() {
                        Ok(text) => match serde_json::from_str::<LedgerApiResponse>(&text) {
                            Ok(ledger_api_response) => Ok((
                                (ledger_api_response.active_stake, ledger_api_response.total_staked),
                                ledger_api_response.decentralisation_param,
                            )),
                            Err(error) => Err(Error::from(error)),
                        },
                        Err(error) => Err(Error::new(ErrorKind::Other, format!("Sigma API Error: {}", error))),
                    },
                    Err(error) => Err(Error::new(ErrorKind::Other, format!("Sigma API Error: {}", error))),
                }
            }
            Err(error) => Err(Error::new(ErrorKind::Other, format!("Sigma API Error: {}", error))),
        }
    } else {
        // Calculate values from json
        let ledger_state = &PathBuf::from(OsString::from_str(ledger_state).unwrap());
        let ledger: Ledger =
            match serde_json::from_reader::<BufReader<File>, Ledger2>(BufReader::new(File::open(ledger_state)?)) {
                Ok(ledger2) => ledger2.nes_es,
                Err(error) => {
                    debug!("Falling back to old ledger state: {:?}", error);
                    serde_json::from_reader(BufReader::new(File::open(ledger_state)?))?
                }
            };

        Ok((
            match ledger_set {
                LedgerSet::Mark => {
                    debug!("Mark");
                    calculate_sigma(ledger.es_snapshots.stake_mark, pool_id)
                }
                LedgerSet::Set => {
                    debug!("Set");
                    calculate_sigma(ledger.es_snapshots.stake_set, pool_id)
                }
                LedgerSet::Go => {
                    debug!("Go");
                    calculate_sigma(ledger.es_snapshots.stake_go, pool_id)
                }
            },
            match ledger_set {
                LedgerSet::Mark => {
                    if !ledger.es_l_state.utxo_state.ppups.proposals.proposal.is_empty()
                        && ledger
                            .es_l_state
                            .utxo_state
                            .ppups
                            .proposals
                            .proposal
                            .iter()
                            .next()
                            .unwrap()
                            .1
                            .decentralisation_param
                            .is_some()
                    {
                        ledger
                            .es_l_state
                            .utxo_state
                            .ppups
                            .proposals
                            .proposal
                            .iter()
                            .next()
                            .unwrap()
                            .1
                            .decentralisation_param
                            .clone()
                            .unwrap()
                    } else {
                        ledger.es_pp.decentralisation_param
                    }
                }
                LedgerSet::Set => ledger.es_pp.decentralisation_param,
                LedgerSet::Go => ledger.es_prev_pp.decentralisation_param,
            },
        ))
    }
}
