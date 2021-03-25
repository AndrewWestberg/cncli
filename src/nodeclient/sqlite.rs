use std::io;

use blake2b_simd::Params;
use cardano_ouroboros_network::{BlockHeader, BlockStore};
use log::{debug, info};
use rusqlite::{named_params, Connection, Error, NO_PARAMS};
use std::path::Path;

pub struct SqLiteBlockStore {
    pub db: Connection,
}

impl SqLiteBlockStore {
    const DB_VERSION: i64 = 3;

    pub fn new(db_path: &Path) -> Result<SqLiteBlockStore, Error> {
        debug!("Opening database");
        let mut db = Connection::open(db_path)?;
        db.execute_batch("PRAGMA journal_mode=WAL")?;

        let tx = db.transaction()?;
        {
            debug!("Intialize database.");
            tx.execute(
                "CREATE TABLE IF NOT EXISTS db_version (version INTEGER PRIMARY KEY)",
                NO_PARAMS,
            )?;
            let mut stmt = tx.prepare("SELECT version FROM db_version")?;
            let mut rows = stmt.query(NO_PARAMS)?;
            let version: i64 = match rows.next()? {
                None => -1,
                Some(row) => row.get(0)?,
            };

            // Upgrade their database to version 1
            if version < 1 {
                info!(" Create database at version 1...");
                tx.execute(
                    "CREATE TABLE IF NOT EXISTS chain (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT, \
                    block_number INTEGER NOT NULL, \
                    slot_number INTEGER NOT NULL, \
                    hash TEXT NOT NULL, \
                    prev_hash TEXT NOT NULL, \
                    eta_v TEXT NOT NULL, \
                    node_vkey TEXT NOT NULL, \
                    node_vrf_vkey TEXT NOT NULL, \
                    eta_vrf_0 TEXT NOT NULL, \
                    eta_vrf_1 TEXT NOT NULL, \
                    leader_vrf_0 TEXT NOT NULL, \
                    leader_vrf_1 TEXT NOT NULL, \
                    block_size INTEGER NOT NULL, \
                    block_body_hash TEXT NOT NULL, \
                    pool_opcert TEXT NOT NULL, \
                    unknown_0 INTEGER NOT NULL, \
                    unknown_1 INTEGER NOT NULL, \
                    unknown_2 TEXT NOT NULL, \
                    protocol_major_version INTEGER NOT NULL, \
                    protocol_minor_version INTEGER NOT NULL, \
                    orphaned INTEGER NOT NULL DEFAULT 0 \
                    )",
                    NO_PARAMS,
                )?;
                tx.execute(
                    "CREATE INDEX IF NOT EXISTS idx_chain_slot_number ON chain(slot_number)",
                    NO_PARAMS,
                )?;
                tx.execute(
                    "CREATE INDEX IF NOT EXISTS idx_chain_orphaned ON chain(orphaned)",
                    NO_PARAMS,
                )?;
                tx.execute("CREATE INDEX IF NOT EXISTS idx_chain_hash ON chain(hash)", NO_PARAMS)?;
                tx.execute(
                    "CREATE INDEX IF NOT EXISTS idx_chain_block_number ON chain(block_number)",
                    NO_PARAMS,
                )?;
            }

            // Upgrade their database to version 2
            if version < 2 {
                info!("Upgrade database to version 2...");
                tx.execute(
                    "CREATE TABLE IF NOT EXISTS slots (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT, \
                    epoch INTEGER NOT NULL, \
                    pool_id TEXT NOT NULL, \
                    slot_qty INTEGER NOT NULL, \
                    slots TEXT NOT NULL, \
                    hash TEXT NOT NULL,
                    UNIQUE(epoch,pool_id)
                )",
                    NO_PARAMS,
                )?;
            }

            if version < 3 {
                info!("Upgrade database to version 3...");
                tx.execute(
                    "CREATE INDEX IF NOT EXISTS idx_chain_node_vkey ON chain(node_vkey)",
                    NO_PARAMS,
                )?;
                tx.execute(
                    "ALTER TABLE chain ADD COLUMN pool_id TEXT NOT NULL DEFAULT ''",
                    NO_PARAMS,
                )?;
                tx.execute(
                    "CREATE INDEX IF NOT EXISTS idx_chain_pool_id ON chain(pool_id)",
                    NO_PARAMS,
                )?;

                let count: i64 = tx.query_row("SELECT COUNT(DISTINCT node_vkey) from chain", NO_PARAMS, |row| {
                    row.get(0)
                })?;

                if count > 0 {
                    let mut stmt = tx.prepare("SELECT DISTINCT node_vkey FROM chain")?;
                    let vkeys = stmt
                        .query_map(NO_PARAMS, |row| {
                            let node_vkey_result: Result<String, Error> = row.get(0);
                            let node_vkey = node_vkey_result?;
                            Ok(node_vkey)
                        })
                        .ok()
                        .unwrap();

                    info!("{} pool id records to process. Please be patient...", &count);
                    for (i, node_vkey) in vkeys.into_iter().enumerate() {
                        let vkey = node_vkey.unwrap();
                        let node_vkey_bytes = hex::decode(&vkey).unwrap();
                        let pool_id = hex::encode(
                            Params::new()
                                .hash_length(28)
                                .to_state()
                                .update(&*node_vkey_bytes)
                                .finalize()
                                .as_bytes()
                                .to_vec(),
                        );

                        tx.execute_named(
                            "UPDATE chain SET pool_id=:pool_id WHERE node_vkey=:node_vkey",
                            named_params! {
                                ":pool_id": pool_id,
                                ":node_vkey": vkey
                            },
                        )?;

                        if i % 25 == 0 {
                            info!("Updated record {} of {}...", i, count);
                        }
                    }
                    info!("Updated record {} of {}...done!", count, count);
                }
            }

            // Update the db version now that we've upgraded the user's database fully
            if version < 0 {
                tx.execute(
                    "INSERT INTO db_version (version) VALUES (?1)",
                    &[&SqLiteBlockStore::DB_VERSION],
                )?;
            } else {
                tx.execute("UPDATE db_version SET version=?1", &[&SqLiteBlockStore::DB_VERSION])?;
            }
        }
        tx.commit()?;

        Ok(SqLiteBlockStore { db })
    }

    fn sql_save_block(
        &mut self,
        pending_blocks: &mut Vec<BlockHeader>,
        network_magic: u32,
    ) -> Result<(), rusqlite::Error> {
        let db = &mut self.db;

        // get the last block eta_v (nonce) in the db
        let mut prev_eta_v = {
            hex::decode(
                match db.query_row(
                    "SELECT eta_v, max(slot_number) FROM chain WHERE orphaned = 0",
                    NO_PARAMS,
                    |row| row.get(0),
                ) {
                    Ok(eta_v) => eta_v,
                    Err(_) => {
                        match network_magic {
                            764824073 => {
                                // mainnet genesis hash
                                info!("Start nonce calculation for mainnet.");
                                String::from("1a3be38bcbb7911969283716ad7aa550250226b76a61fc51cc9a9a35d9276d81")
                            }
                            1097911063 => {
                                // Testnet genesis hash
                                info!("Start nonce calculation for testnet.");
                                String::from("849a1764f152e1b09c89c0dfdbcbdd38d711d1fec2db5dfa0f87cf2737a0eaf4")
                            }
                            3 => {
                                // Launchpad genesis hash
                                info!("Start nonce calculation for launchpad.");
                                String::from("8587fca9128b0470dcaf928f00bb2bd99dec5047e080a2da3aa419bd17023d75")
                            }
                            12 => {
                                // allegra genesis hash
                                info!("Start nonce calculation for allegra testnet.");
                                String::from("47daa6201f436c90f9c76e343e0fd6536262b7ca2455ec306aa2fcc45c97bb4d")
                            }
                            141 => {
                                // guild genesis hash
                                info!("Start nonce calculation for guild testnet.");
                                String::from("24c22740688a4bb783b3f8dbbaced2ecb661c3ffc3defbc3bed6157c055e36cf")
                            }
                            _ => {
                                panic!("Unknown genesis hash for network_magic {}", network_magic);
                            }
                        }
                    }
                },
            )
            .unwrap()
        };

        let tx = db.transaction()?;
        {
            // scope for db transaction
            let mut orphan_stmt = tx.prepare("UPDATE chain SET orphaned = 1 WHERE block_number >= ?1")?;
            let mut insert_stmt = tx.prepare(
                "INSERT INTO chain (\
            block_number, \
            slot_number, \
            hash, \
            prev_hash, \
            pool_id, \
            eta_v, \
            node_vkey, \
            node_vrf_vkey, \
            eta_vrf_0, \
            eta_vrf_1, \
            leader_vrf_0, \
            leader_vrf_1, \
            block_size, \
            block_body_hash, \
            pool_opcert, \
            unknown_0, \
            unknown_1, \
            unknown_2, \
            protocol_major_version, \
            protocol_minor_version) \
            VALUES (\
            :block_number, \
            :slot_number, \
            :hash, \
            :prev_hash, \
            :pool_id, \
            :eta_v, \
            :node_vkey, \
            :node_vrf_vkey, \
            :eta_vrf_0, \
            :eta_vrf_1, \
            :leader_vrf_0, \
            :leader_vrf_1, \
            :block_size, \
            :block_body_hash, \
            :pool_opcert, \
            :unknown_0, \
            :unknown_1, \
            :unknown_2, \
            :protocol_major_version, \
            :protocol_minor_version)",
            )?;

            for block in pending_blocks.drain(..) {
                // Set any necessary blocks as orphans
                let orphan_num = orphan_stmt.execute(&[&block.block_number])?;

                if orphan_num > 0 {
                    // get the last block eta_v (nonce) in the db
                    prev_eta_v = {
                        hex::decode(
                            match tx.query_row(
                                "SELECT eta_v, max(slot_number) FROM chain WHERE orphaned = 0",
                                NO_PARAMS,
                                |row| row.get(0),
                            ) {
                                Ok(eta_v) => eta_v,
                                Err(_) => {
                                    match network_magic {
                                        764824073 => {
                                            // mainnet genesis hash
                                            String::from(
                                                "1a3be38bcbb7911969283716ad7aa550250226b76a61fc51cc9a9a35d9276d81",
                                            )
                                        }
                                        141 => {
                                            // guild genesis hash
                                            String::from(
                                                "24c22740688a4bb783b3f8dbbaced2ecb661c3ffc3defbc3bed6157c055e36cf",
                                            )
                                        }
                                        _ => {
                                            // assume testnet genesis hash
                                            String::from(
                                                "849a1764f152e1b09c89c0dfdbcbdd38d711d1fec2db5dfa0f87cf2737a0eaf4",
                                            )
                                        }
                                    }
                                }
                            },
                        )
                        .unwrap()
                    };
                }
                // blake2b hash of eta_vrf_0
                let mut block_eta_v = Params::new()
                    .hash_length(32)
                    .to_state()
                    .update(&*block.eta_vrf_0)
                    .finalize()
                    .as_bytes()
                    .to_vec();
                prev_eta_v.append(&mut block_eta_v);
                // blake2b hash of prev_eta_v + block_eta_v
                prev_eta_v = Params::new()
                    .hash_length(32)
                    .to_state()
                    .update(&*prev_eta_v)
                    .finalize()
                    .as_bytes()
                    .to_vec();

                // blake2b 224 of node_vkey is the pool_id
                let pool_id = Params::new()
                    .hash_length(28)
                    .to_state()
                    .update(&*block.node_vkey)
                    .finalize()
                    .as_bytes()
                    .to_vec();

                insert_stmt.execute_named(named_params! {
                    ":block_number" : block.block_number,
                    ":slot_number": block.slot_number,
                    ":hash" : hex::encode(block.hash),
                    ":prev_hash" : hex::encode(block.prev_hash),
                    ":pool_id" : hex::encode(pool_id),
                    ":eta_v" : hex::encode(&prev_eta_v),
                    ":node_vkey" : hex::encode(block.node_vkey),
                    ":node_vrf_vkey" : hex::encode(block.node_vrf_vkey),
                    ":eta_vrf_0" : hex::encode(block.eta_vrf_0),
                    ":eta_vrf_1" : hex::encode(block.eta_vrf_1),
                    ":leader_vrf_0" : hex::encode(block.leader_vrf_0),
                    ":leader_vrf_1" : hex::encode(block.leader_vrf_1),
                    ":block_size" : block.block_size,
                    ":block_body_hash" : hex::encode(block.block_body_hash),
                    ":pool_opcert" : hex::encode(block.pool_opcert),
                    ":unknown_0" : block.unknown_0,
                    ":unknown_1" : block.unknown_1,
                    ":unknown_2" : hex::encode(block.unknown_2),
                    ":protocol_major_version" : block.protocol_major_version,
                    ":protocol_minor_version" : block.protocol_minor_version,
                })?;
            }
        }

        tx.commit()?;
        Ok(())
    }
}

impl BlockStore for SqLiteBlockStore {
    fn save_block(&mut self, mut pending_blocks: &mut Vec<BlockHeader>, network_magic: u32) -> io::Result<()> {
        match self.sql_save_block(&mut pending_blocks, network_magic) {
            Ok(_) => Ok(()),
            Err(_) => Err(io::Error::new(io::ErrorKind::Other, "Database error!")),
        }
    }

    fn load_blocks(&mut self) -> Option<Vec<(i64, Vec<u8>)>> {
        let db = &self.db;
        let mut stmt = db
            .prepare("SELECT slot_number, hash FROM chain where orphaned = 0 ORDER BY slot_number DESC LIMIT 33")
            .unwrap();
        let blocks = stmt
            .query_map(NO_PARAMS, |row| {
                let slot_result: Result<i64, Error> = row.get(0);
                let hash_result: Result<String, Error> = row.get(1);
                let slot = slot_result?;
                let hash = hash_result?;
                Ok((slot, hex::decode(hash).unwrap()))
            })
            .ok()?;
        Some(blocks.map(|item| item.unwrap()).collect())
    }
}
