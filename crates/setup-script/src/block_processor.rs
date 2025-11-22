use std::sync::Arc;
use zebra_chain::{block::Block, transparent::Input};

pub struct ProcessedBlock {
    pub utxo_updates: Vec<UtxoUpdate>,
    pub nullifiers: Vec<[u8; 32]>,
}

pub enum UtxoUpdate {
    Create {
        txid: zebra_chain::transaction::Hash,
        vout: u32,
        value: i64,
        lock_script: Vec<u8>,
    },
    Spend {
        txid: zebra_chain::transaction::Hash,
        vout: u32,
    },
}

pub fn process_block_sync(block: Arc<Block>) -> ProcessedBlock {
    // Pre-allocate with estimated capacity to reduce allocations
    let tx_count = block.transactions.len();
    let mut utxo_updates = Vec::with_capacity(tx_count * 4); // rough estimate
    let mut nullifiers = Vec::with_capacity(tx_count * 2);

    for tx in block.transactions.iter() {
        let txid = tx.hash();

        // Process inputs (spend UTXOs)
        for input in tx.inputs() {
            if let Input::PrevOut { outpoint, .. } = input {
                utxo_updates.push(UtxoUpdate::Spend {
                    txid: outpoint.hash,
                    vout: outpoint.index,
                });
            }
        }

        // Process outputs (create UTXOs)
        for (index, output) in tx.outputs().iter().enumerate() {
            let value: i64 = i64::from(output.value());
            let lock_script = output.lock_script.as_raw_bytes().to_vec();

            utxo_updates.push(UtxoUpdate::Create {
                txid,
                vout: index as u32,
                value,
                lock_script,
            });
        }

        // Collect nullifiers from shielded data
        // Sprout nullifiers
        for joinsplit in tx.sprout_groth16_joinsplits() {
            nullifiers.push(joinsplit.nullifiers[0].0.0);
            nullifiers.push(joinsplit.nullifiers[1].0.0);
        }

        // Sapling nullifiers
        for nullifier in tx.sapling_nullifiers() {
            nullifiers.push(nullifier.0.0);
        }

        // Orchard nullifiers
        for nullifier in tx.orchard_nullifiers() {
            nullifiers.push((*nullifier).into());
        }
    }

    ProcessedBlock {
        utxo_updates,
        nullifiers,
    }
}
