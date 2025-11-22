use zebra_state::{ZebraDb, state_database_format_version_in_code, IntoDisk};
use zebra_chain::parameters::Network;
use zebra_chain::block::Height;

fn main() {
    let cache_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/home/ubuntu/zebra/zebra".to_string());
    
    let config = zebra_state::Config {
        cache_dir: std::path::PathBuf::from(cache_dir),
        ..Default::default()
    };
    
    let db = ZebraDb::new(
        &config,
        "state",
        &state_database_format_version_in_code(),
        &Network::Mainnet,
        false,
        [],
        true,
    );
    
    if let Some(tip_height) = db.finalized_tip_height() {
        println!("Tip height: {}", tip_height.0);
        
        // Get current Orchard count
        let current_count = if let Some(orchard_tree) = db.orchard_tree_by_height(&tip_height) {
            let count = orchard_tree.count();
            let root = orchard_tree.root();
            println!("Orchard count: {}", count);
            println!("Orchard root: {}", hex::encode(root.as_bytes()));
            Some(count)
        } else {
            eprintln!("No Orchard tree found at tip");
            None
        };
        
        // Calculate blocks per day: Zcash has ~1 block per 75 seconds = ~1152 blocks/day
        // 7 days ago = ~8064 blocks
        const BLOCKS_PER_DAY: u32 = 1152;
        const DAYS: u32 = 7;
        let blocks_ago = BLOCKS_PER_DAY * DAYS;
        let past_height = tip_height.0.saturating_sub(blocks_ago);
        
        println!("\n--- Historical Analysis (Past {} days) ---", DAYS);
        println!("Querying height {} ({} blocks ago)...", past_height, blocks_ago);
        
        // Try to get Orchard count from 7 days ago
        let past_count = db.orchard_tree_by_height(&Height(past_height))
            .map(|tree| tree.count());
        
        if let (Some(current), Some(past)) = (current_count, past_count) {
            let diff = current as i64 - past as i64;
            let blocks_diff = tip_height.0 - past_height;
            let rate_per_block = diff as f64 / blocks_diff as f64;
            let rate_per_day = rate_per_block * BLOCKS_PER_DAY as f64;
            let avg_rate_per_day = diff as f64 / DAYS as f64;
            
            println!("Orchard count {} days ago: {}", DAYS, past);
            println!("Change over {} days: {} commitments", DAYS, diff);
            println!("Average rate: {:.2} commitments/day", avg_rate_per_day);
            println!("Rate per block: {:.4} commitments/block", rate_per_block);
            println!("Extrapolated daily rate: {:.2} commitments/day", rate_per_day);
            
            // Calculate 2^16 shard timing
            const SHARD_SIZE: u64 = 1 << 16; // 65,536
            let time_to_fill_shard_days = SHARD_SIZE as f64 / avg_rate_per_day;
            let time_to_fill_shard_hours = time_to_fill_shard_days * 24.0;
            
            println!("\n--- 2^16 Shard Analysis ---");
            println!("Shard size: {} positions (2^16)", SHARD_SIZE);
            println!("Average time to fill one shard: {:.2} days ({:.1} hours)", 
                     time_to_fill_shard_days, time_to_fill_shard_hours);
            
            // Calculate progress to next shard boundary
            let current_pos = current as u64;
            let current_shard = current_pos / SHARD_SIZE;
            let next_shard_boundary = (current_shard + 1) * SHARD_SIZE;
            let positions_remaining = next_shard_boundary - current_pos;
            let progress_in_shard = current_pos % SHARD_SIZE;
            let progress_percent = (progress_in_shard as f64 / SHARD_SIZE as f64) * 100.0;
            let days_to_next_shard = positions_remaining as f64 / avg_rate_per_day;
            let hours_to_next_shard = days_to_next_shard * 24.0;
            
            println!("\n--- Progress to Next Shard Boundary ---");
            println!("Current position: {}", current_pos);
            println!("Current shard: {} (positions {} to {})", 
                     current_shard, current_shard * SHARD_SIZE, (current_shard + 1) * SHARD_SIZE - 1);
            println!("Next shard boundary: {} (shard {})", next_shard_boundary, current_shard + 1);
            println!("Positions remaining: {}", positions_remaining);
            println!("Progress in current shard: {:.2}% ({}/{} positions)", 
                     progress_percent, progress_in_shard, SHARD_SIZE);
            println!("Time to next shard: {:.2} days ({:.1} hours)", 
                     days_to_next_shard, hours_to_next_shard);
        } else {
            println!("Could not retrieve historical Orchard count at height {}", past_height);
            println!("(Zebrad may not store historical tree states)");
        }
        
        // Sapling info
        if let Some(sapling_tree) = db.sapling_tree_by_height(&tip_height) {
            let count = sapling_tree.count();
            let root = sapling_tree.root();
            println!("\nSapling count: {}", count);
            println!("Sapling root: {}", hex::encode(root.as_bytes()));
        }
    } else {
        eprintln!("No finalized tip found");
    }
}

