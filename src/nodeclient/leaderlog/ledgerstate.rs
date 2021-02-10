use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Error};
use std::path::PathBuf;

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
    ledger_state: &PathBuf,
    ledger_set: &LedgerSet,
    pool_id: &str,
) -> Result<((u64, u64), Rational), Error> {
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
