pub mod nodeclient {
    use std::fs::File;
    use std::io::{stdout, BufReader};
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::string::ParseError;
    use std::thread;
    use std::thread::JoinHandle;

    use serde::Deserialize;
    use structopt::StructOpt;

    use crate::nodeclient::leaderlog::handle_error;

    pub mod leaderlog;
    pub mod math;
    pub mod ping;
    pub mod pooltool;
    pub mod sqlite;
    pub mod sync;
    mod validate;

    pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

    #[derive(Debug)]
    pub enum LedgerSet {
        Mark,
        Set,
        Go,
    }

    impl FromStr for LedgerSet {
        type Err = ParseError;
        fn from_str(ledger_set: &str) -> Result<Self, Self::Err> {
            match ledger_set {
                "next" => Ok(LedgerSet::Mark),
                "current" => Ok(LedgerSet::Set),
                "prev" => Ok(LedgerSet::Go),
                _ => Ok(LedgerSet::Set),
            }
        }
    }

    #[derive(Debug, StructOpt)]
    pub enum Command {
        Ping {
            #[structopt(short, long, help = "cardano-node hostname to connect to")]
            host: String,
            #[structopt(short, long, default_value = "3001", help = "cardano-node port")]
            port: u16,
            #[structopt(long, default_value = "764824073", help = "network magic.")]
            network_magic: u32,
        },
        Validate {
            #[structopt(long, help = "full or partial block hash to validate")]
            hash: String,
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
        },
        Sync {
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
            #[structopt(short, long, help = "cardano-node hostname to connect to")]
            host: String,
            #[structopt(short, long, default_value = "3001", help = "cardano-node port")]
            port: u16,
            #[structopt(long, default_value = "764824073", help = "network magic.")]
            network_magic: u32,
            #[structopt(long, help = "Exit at 100% sync'd.")]
            no_service: bool,
        },
        Leaderlog {
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "byron genesis json file")]
            byron_genesis: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "shelley genesis json file")]
            shelley_genesis: std::path::PathBuf,
            #[structopt(long, help = "pool active stake snapshot value in lovelace")]
            pool_stake: Option<u64>,
            #[structopt(long, help = "total active stake snapshot value in lovelace")]
            active_stake: Option<u64>,
            #[structopt(long, help = "hex string of the extra entropy value")]
            extra_entropy: Option<String>,
            #[structopt(
                long,
                help = "ledger state json file or API url",
                default_value = "https://api.crypto2099.io/v1/sigma"
            )]
            ledger_state: String,
            #[structopt(
                long,
                default_value = "current",
                help = "Which ledger data to use. prev - previous epoch, current - current epoch, next - future epoch"
            )]
            ledger_set: LedgerSet,
            #[structopt(long, help = "lower-case hex pool id")]
            pool_id: String,
            #[structopt(parse(from_os_str), long, help = "pool's vrf.skey file")]
            pool_vrf_skey: std::path::PathBuf,
            #[structopt(
                long = "tz",
                default_value = "America/Los_Angeles",
                help = "TimeZone string from the IANA database - https://en.wikipedia.org/wiki/List_of_tz_database_time_zones"
            )]
            timezone: String,
        },
        Sendtip {
            #[structopt(
                parse(from_os_str),
                long,
                default_value = "./pooltool.json",
                help = "pooltool config file for sending tips"
            )]
            config: std::path::PathBuf,
            #[structopt(
                parse(from_os_str),
                long,
                help = "path to cardano-node executable for gathering version info"
            )]
            cardano_node: std::path::PathBuf,
        },
        Sendslots {
            #[structopt(
                parse(from_os_str),
                long,
                default_value = "./pooltool.json",
                help = "pooltool config file for sending slots"
            )]
            config: std::path::PathBuf,
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "byron genesis json file")]
            byron_genesis: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "shelley genesis json file")]
            shelley_genesis: std::path::PathBuf,
        },
        Status {
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "byron genesis json file")]
            byron_genesis: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "shelley genesis json file")]
            shelley_genesis: std::path::PathBuf,
        },
        Nonce {
            #[structopt(
                parse(from_os_str),
                short,
                long,
                default_value = "./cncli.db",
                help = "sqlite database file"
            )]
            db: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "byron genesis json file")]
            byron_genesis: std::path::PathBuf,
            #[structopt(parse(from_os_str), long, help = "shelley genesis json file")]
            shelley_genesis: std::path::PathBuf,
            #[structopt(long, help = "hex string of the extra entropy value")]
            extra_entropy: Option<String>,
            #[structopt(
                long,
                help = "ledger state json file or API url",
                default_value = "https://api.crypto2099.io/v1/entropy"
            )]
            ledger_state: String,
            #[structopt(
                long,
                default_value = "current",
                help = "Which ledger data to use. prev - previous epoch, current - current epoch, next - future epoch"
            )]
            ledger_set: LedgerSet,
        },
    }

    pub fn start(cmd: Command) {
        match cmd {
            Command::Ping {
                ref host,
                ref port,
                ref network_magic,
            } => {
                ping::ping(&mut stdout(), host.as_str(), *port, *network_magic);
            }
            Command::Validate { ref db, ref hash } => {
                validate::validate_block(db, hash.as_str());
            }
            Command::Sync {
                ref db,
                ref host,
                ref port,
                ref network_magic,
                ref no_service,
            } => {
                sync::sync(db, host.as_str(), *port, *network_magic, *no_service);
            }
            Command::Leaderlog {
                ref db,
                ref byron_genesis,
                ref shelley_genesis,
                ref pool_stake,
                ref active_stake,
                ref extra_entropy,
                ref ledger_state,
                ref ledger_set,
                ref pool_id,
                ref pool_vrf_skey,
                ref timezone,
            } => {
                leaderlog::calculate_leader_logs(
                    db,
                    byron_genesis,
                    shelley_genesis,
                    pool_stake,
                    active_stake,
                    extra_entropy,
                    ledger_state,
                    ledger_set,
                    pool_id,
                    pool_vrf_skey,
                    timezone,
                    false,
                );
            }
            Command::Nonce {
                ref db,
                ref byron_genesis,
                ref shelley_genesis,
                ref extra_entropy,
                ref ledger_state,
                ref ledger_set,
            } => leaderlog::calculate_leader_logs(
                db,
                byron_genesis,
                shelley_genesis,
                &None,
                &None,
                extra_entropy,
                ledger_state,
                ledger_set,
                &String::from("nonce"),
                &PathBuf::new(),
                &"America/Los_Angeles".to_string(),
                true,
            ),
            Command::Sendtip {
                ref config,
                ref cardano_node,
            } => {
                if !config.exists() {
                    handle_error("config not found!");
                    return;
                }
                if !cardano_node.exists() {
                    handle_error("cardano-node not found!");
                    return;
                }

                let pooltool_config: PooltoolConfig = get_pooltool_config(config);
                let mut handles: Vec<JoinHandle<_>> = vec![];
                for pool in pooltool_config.pools.into_iter() {
                    let api_key = pooltool_config.api_key.clone();
                    let cardano_node_path = cardano_node.clone();
                    handles.push(thread::spawn(move || {
                        sync::sendtip(
                            pool.name,
                            pool.pool_id,
                            pool.host,
                            pool.port,
                            api_key,
                            &*cardano_node_path,
                        );
                    }));
                }

                for handle in handles {
                    handle.join().unwrap()
                }
            }
            Command::Sendslots {
                ref config,
                ref db,
                ref byron_genesis,
                ref shelley_genesis,
            } => {
                if !config.exists() {
                    handle_error("config not found!");
                    return;
                }
                let pooltool_config: PooltoolConfig = get_pooltool_config(config);
                leaderlog::send_slots(db, byron_genesis, shelley_genesis, pooltool_config);
            }
            Command::Status {
                ref db,
                ref byron_genesis,
                ref shelley_genesis,
            } => {
                leaderlog::status(db, byron_genesis, shelley_genesis);
            }
        }
    }

    fn get_pooltool_config(config: &Path) -> PooltoolConfig {
        let buf = BufReader::new(File::open(config).unwrap());
        serde_json::from_reader(buf).unwrap()
    }

    #[derive(Debug, Deserialize)]
    pub struct PooltoolConfig {
        api_key: String,
        pools: Vec<Pool>,
    }

    #[derive(Debug, Deserialize)]
    struct Pool {
        name: String,
        pool_id: String,
        host: String,
        port: u16,
    }
}
