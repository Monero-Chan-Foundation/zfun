#![no_main]

use zfun_lib::{track_cycles, verify_holdings, HoldingsWitness, PROOF_DOMAIN, SNAPSHOT};

monerochan_runtime::entrypoint!(main);

pub fn main() {
    let _force_new_vk = 1; // Force new VK for testing
    // Read witness - print size before and after for debugging
    let witness = {
        println!("monerochan: About to read HoldingsWitness from stdin");
        let w: HoldingsWitness = monerochan_runtime::io::read();
        println!("monerochan: Successfully deserialized {} accounts, nonce={}", w.accounts.len(), w.nonce);
        w
    };

    // Use actual_snapshot from witness if available, otherwise fall back to hardcoded SNAPSHOT
    let snapshot_to_use = witness.actual_snapshot.as_ref().unwrap_or(&SNAPSHOT);

    match verify_holdings(&witness, snapshot_to_use, PROOF_DOMAIN, true) {
        Ok(result) => {
            monerochan_runtime::io::commit(&result);
        }
        Err(e) => {
            panic!("verify_holdings failed: {e}");
        }
    }
}
