use std::path::Path;
use std::time::Duration;

use async_std::task;
use cardano_ouroboros_network::{
    mux,
    protocols::chainsync::{ChainSyncProtocol, Mode},
    BlockHeader,
};
use futures::{executor::block_on, try_join};
use log::{error, info};

use crate::nodeclient::pooltool;
use crate::nodeclient::sqlite;
use cardano_ouroboros_network::protocols::chainsync::Listener;

struct SyncExit {}

impl Listener for SyncExit {
    fn handle_tip(&mut self, _msg_roll_forward: &BlockHeader) {
        info!("Exiting...");
        std::process::exit(0);
    }
}

pub(crate) fn sync(db: &Path, host: &str, port: u16, network_magic: u32, no_service: bool) {
    block_on(async {
        loop {
            // Retry to establish connection forever
            let block_store = sqlite::SqLiteBlockStore::new(db).unwrap();
            match mux::connection::connect(host, port).await {
                Ok(channel) => match channel.handshake(network_magic).await {
                    Ok(_) => {
                        let chain_sync_protocol = if no_service {
                            ChainSyncProtocol {
                                mode: Mode::Sync,
                                network_magic,
                                store: Some(Box::new(block_store)),
                                notify: Some(Box::new(SyncExit {})),
                                ..Default::default()
                            }
                        } else {
                            ChainSyncProtocol {
                                mode: Mode::Sync,
                                network_magic,
                                store: Some(Box::new(block_store)),
                                ..Default::default()
                            }
                        };
                        match try_join!(channel.execute(chain_sync_protocol),) {
                            Ok(_) => {}
                            Err(error) => {
                                error!("{}", error);
                            }
                        }
                    }
                    Err(error) => {
                        error!("{}", error);
                    }
                },
                Err(error) => {
                    error!("{:?}", error);
                }
            }

            task::sleep(Duration::from_secs(5)).await;
        }
    });
}

pub(crate) fn sendtip(
    pool_name: String,
    pool_id: String,
    host: String,
    port: u16,
    api_key: String,
    cardano_node_path: &Path,
) {
    block_on(async {
        loop {
            let pooltool_notifier = pooltool::PoolToolNotifier {
                pool_name: pool_name.clone(),
                pool_id: pool_id.clone(),
                api_key: api_key.clone(),
                cardano_node_path: cardano_node_path.to_path_buf(),
                ..Default::default()
            };
            match mux::connection::connect(&*host, port).await {
                Ok(channel) => {
                    match channel.handshake(764824073).await {
                        Ok(_) => {
                            match try_join!(channel.execute({
                                ChainSyncProtocol {
                                    mode: Mode::SendTip,
                                    network_magic: 764824073, // hardcoded to mainnet for pooltool
                                    notify: Some(Box::new(pooltool_notifier)),
                                    ..Default::default()
                                }
                            }),)
                            {
                                Ok(_) => {}
                                Err(error) => {
                                    error!("{}", error);
                                }
                            }
                        }
                        Err(error) => {
                            error!("{}", error);
                        }
                    }
                }
                Err(error) => {
                    error!("{:?}", error);
                }
            }

            task::sleep(Duration::from_secs(5)).await;
        }
    });
}
