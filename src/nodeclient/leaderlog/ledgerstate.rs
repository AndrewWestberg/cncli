use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind};
use std::path::PathBuf;
use std::str::FromStr;

use log::debug;
use rug::Rational;
use serde::Deserialize;

use crate::nodeclient::leaderlog::deserialize::rational;
use crate::nodeclient::{LedgerSet, APP_USER_AGENT};

#[derive(Debug, Deserialize)]
pub struct Ledger3 {
    #[serde(alias = "stateBefore", alias = "nesEs")]
    pub state_before: Ledger,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
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

    extra_entropy: ExtraEntropy,
}

#[derive(Debug, Deserialize)]
struct EsLState {
    #[serde(alias = "_utxoState", alias = "utxoState")]
    utxo_state: UtxoState,
}

#[derive(Debug, Deserialize)]
struct UtxoState {
    #[serde(alias = "_ppups", alias = "ppups")]
    ppups: Ppups,
}

#[derive(Debug, Deserialize)]
struct Ppups {
    proposals: Proposals,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Proposals {
    ProposalsV1(ProposalsOld),
    ProposalsV2(Vec<Vec<ProposalsNew>>),
}

#[derive(Debug, Deserialize)]
struct ProposalsOld {
    #[serde(flatten)]
    proposal: HashMap<String, Proposal>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProposalsNew {
    ProposalId(String),
    ProposalParams(Proposal),
}

#[derive(Debug, Deserialize)]
struct Proposal {
    #[serde(alias = "_d", alias = "decentralisationParam")]
    decentralisation_param: Option<f64>,

    #[serde(alias = "_extraEntropy", alias = "extraEntropy")]
    extra_entropy: Option<ExtraEntropy>,
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraEntropy {
    tag: String,
    contents: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EsSnapshots {
    #[serde(alias = "_pstakeMark", alias = "pstakeMark")]
    stake_mark: StakeGroup,
    #[serde(alias = "_pstakeSet", alias = "pstakeSet")]
    stake_set: StakeGroup,
    #[serde(alias = "_pstakeGo", alias = "pstakeGo")]
    stake_go: StakeGroup,
}

#[derive(Debug, Deserialize)]
struct StakeGroup {
    #[serde(alias = "_stake", alias = "stake")]
    stake: Vec<Vec<Stake>>,
    #[serde(alias = "_delegations", alias = "delegations")]
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
    #[serde(default)]
    decentralisation_param: Rational,
    active_stake: Option<u64>,
    #[serde(default)]
    total_staked: u64,
    entropy: Option<String>,
    // {"d":"0.16","total_staked":"22369166376492895","active_stake":"8193623134725","sigma":0.00036629094695882095, "entropy:null, "nonce":"6de5370ca56cd7ff8cbca5ddf216f345417708b2a12b8b8c61ac73c3733cce57"}
}

#[derive(Debug)]
pub(crate) struct LedgerInfo {
    pub(crate) sigma: (u64, u64),
    pub(crate) decentralization: Rational,
    pub(crate) extra_entropy: Option<String>,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn calculate_ledger_state_sigma_d_and_extra_entropy(
    pool_stake: &Option<u64>,
    active_stake: &Option<u64>,
    extra_entropy: &Option<String>,
    ledger_state: &str,
    ledger_set: &LedgerSet,
    pool_id: &str,
    epoch: i64,
    is_ledger_api: bool,
    is_just_nonce: bool,
) -> Result<LedgerInfo, Error> {
    if is_ledger_api {
        if is_just_nonce && extra_entropy.is_some() {
            // no apis, we're just providing some extra entropy for nonce calculation
            Ok(LedgerInfo {
                sigma: (0, 1),
                decentralization: Rational::from(0),
                extra_entropy: extra_entropy.clone(),
            })
        } else if pool_stake.is_some() {
            // We're assuming d=0 at this point if we're using this new cardano-cli stake-snapshot API
            Ok(LedgerInfo {
                sigma: (pool_stake.unwrap(), active_stake.unwrap()),
                decentralization: Rational::from(0),
                extra_entropy: extra_entropy.clone(),
            })
        } else {
            match reqwest::blocking::Client::builder().user_agent(APP_USER_AGENT).build() {
                Ok(client) => {
                    let mut url = ledger_state.to_owned();
                    if !is_just_nonce {
                        url.push('/');
                        url.push_str(pool_id);
                    }
                    url.push('/');
                    url.push_str(&*epoch.to_string());
                    let api_result = client.get(&url).send();

                    match api_result {
                        Ok(response) => match response.text() {
                            Ok(text) => match serde_json::from_str::<LedgerApiResponse>(&text) {
                                Ok(ledger_api_response) => match ledger_api_response.active_stake {
                                    Some(active_stake) => Ok(LedgerInfo {
                                        sigma: (active_stake, ledger_api_response.total_staked),
                                        decentralization: ledger_api_response.decentralisation_param,
                                        extra_entropy: ledger_api_response.entropy,
                                    }),
                                    None => {
                                        if !is_just_nonce {
                                            Err(Error::new(
                                                ErrorKind::Other,
                                                "Remote API Error: No active stake found for pool!",
                                            ))
                                        } else {
                                            Ok(LedgerInfo {
                                                sigma: (0, 0),
                                                decentralization: ledger_api_response.decentralisation_param,
                                                extra_entropy: ledger_api_response.entropy,
                                            })
                                        }
                                    }
                                },
                                Err(error) => Err(Error::from(error)),
                            },
                            Err(error) => Err(Error::new(ErrorKind::Other, format!("Remote API Error: {}", error))),
                        },
                        Err(error) => Err(Error::new(ErrorKind::Other, format!("Remote API Error: {}", error))),
                    }
                }
                Err(error) => Err(Error::new(ErrorKind::Other, format!("Remote API Error: {}", error))),
            }
        }
    } else {
        // Calculate values from json
        let ledger_state = &PathBuf::from(OsString::from_str(ledger_state).unwrap());
        let ledger: Ledger =
            match serde_json::from_reader::<BufReader<File>, Ledger3>(BufReader::new(File::open(ledger_state)?)) {
                Ok(ledger3) => ledger3.state_before,
                Err(error) => {
                    debug!("Falling back to old ledger state: {:?}", error);
                    serde_json::from_reader(BufReader::new(File::open(ledger_state)?))?
                }
            };

        Ok(LedgerInfo {
            sigma: match ledger_set {
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
            decentralization: match ledger_set {
                LedgerSet::Mark => match &ledger.es_l_state.utxo_state.ppups.proposals {
                    Proposals::ProposalsV1(proposals) => {
                        if !proposals.proposal.is_empty()
                            && proposals
                                .proposal
                                .iter()
                                .next()
                                .unwrap()
                                .1
                                .decentralisation_param
                                .is_some()
                        {
                            Rational::from_f64(
                                proposals
                                    .proposal
                                    .iter()
                                    .next()
                                    .unwrap()
                                    .1
                                    .decentralisation_param
                                    .clone()
                                    .unwrap(),
                            )
                            .unwrap()
                        } else {
                            ledger.es_pp.decentralisation_param
                        }
                    }
                    Proposals::ProposalsV2(proposals) => {
                        let mut decentralisation_param: Rational = ledger.es_pp.decentralisation_param;
                        match proposals.first() {
                            None => {}
                            Some(v) => {
                                for proposals in v.iter() {
                                    match proposals {
                                        ProposalsNew::ProposalId(_) => {}
                                        ProposalsNew::ProposalParams(proposal) => {
                                            if proposal.decentralisation_param.is_some() {
                                                decentralisation_param = Rational::from_f64(
                                                    proposal.decentralisation_param.clone().unwrap(),
                                                )
                                                .unwrap();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        decentralisation_param
                    }
                },
                LedgerSet::Set => ledger.es_pp.decentralisation_param,
                LedgerSet::Go => ledger.es_prev_pp.decentralisation_param,
            },
            extra_entropy: match ledger_set {
                LedgerSet::Mark => match &ledger.es_l_state.utxo_state.ppups.proposals {
                    Proposals::ProposalsV1(proposals) => {
                        if !proposals.proposal.is_empty()
                            && proposals.proposal.iter().next().unwrap().1.extra_entropy.is_some()
                        {
                            proposals
                                .proposal
                                .iter()
                                .next()
                                .unwrap()
                                .1
                                .extra_entropy
                                .clone()
                                .unwrap()
                                .contents
                        } else {
                            ledger.es_pp.extra_entropy.contents
                        }
                    }
                    Proposals::ProposalsV2(proposals) => {
                        let mut extra_entropy: Option<String> = ledger.es_pp.extra_entropy.contents;
                        match proposals.first() {
                            None => {}
                            Some(v) => {
                                for proposals in v.iter() {
                                    match proposals {
                                        ProposalsNew::ProposalId(_) => {}
                                        ProposalsNew::ProposalParams(proposal) => {
                                            if proposal.extra_entropy.is_some() {
                                                extra_entropy = proposal.extra_entropy.clone().unwrap().contents;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        extra_entropy
                    }
                },
                LedgerSet::Set => ledger.es_pp.extra_entropy.contents,
                LedgerSet::Go => ledger.es_prev_pp.extra_entropy.contents,
            },
        })
    }
}
