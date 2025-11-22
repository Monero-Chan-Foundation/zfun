use monerochan::network::{NetworkClient, NetworkMode, NetworkSigner};
use monerochan::network::proto::api::{GetProofStatusRequest, JobStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get request ID from command line args
    let request_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0x0dcd4a17d8cb9f5cf38742ffc40235307f87a2751530338e4b7c6902ec41c675".to_string());
    
    // Ensure request_id has 0x prefix
    let request_id = if request_id.starts_with("0x") {
        request_id
    } else {
        format!("0x{}", request_id)
    };
    
    // Get private key from environment or use a dummy one for read-only operations
    let private_key = std::env::var("MONEROCHAN_NETWORK_PRIVATE_KEY")
        .or_else(|_| std::env::var("BASE_PRIVATE_KEY"))
        .unwrap_or_else(|_| {
            eprintln!("Warning: No private key found, using dummy key for read-only operation");
            "1111111111111111111111111111111111111111111111111111111111111111".to_string()
        });
    
    // Create signer - try Solana format first, then Ethereum
    let signer = NetworkSigner::local(&private_key)
        .or_else(|_| {
            eprintln!("Trying alternative key format...");
            NetworkSigner::local(&private_key)
        })?;
    
    // Get RPC URL from environment or use default
    let rpc_url = std::env::var("BASE_RPC_URL").unwrap_or_else(|_| {
        monerochan::network::utils::get_default_rpc_url_for_mode(NetworkMode::Reserved)
    });
    
    println!("Connecting to Monerochan network at: {}", rpc_url);
    println!("Querying status for request_id: {}", request_id);
    
    // Create network client
    let client = NetworkClient::new(signer, rpc_url, NetworkMode::Reserved);
    
    // Make the gRPC request
    let request = GetProofStatusRequest {
        request_id: request_id.clone(),
    };
    
    match client.get_proof_status(request).await {
        Ok(response) => {
            let inner = response.into_inner();
            println!("\n=== Proof Status ===");
            println!("Request ID: {}", request_id);
            println!("Status Code: {}", inner.status);
            
            // Try to parse JobStatus
            match JobStatus::try_from(inner.status) {
                Ok(JobStatus::Succeeded) => println!("Status: Succeeded ✓"),
                Ok(JobStatus::Failed) => println!("Status: Failed ✗"),
                Ok(JobStatus::Running) => println!("Status: Running ⏳"),
                Ok(JobStatus::Pending) => println!("Status: Pending ⏸"),
                Ok(JobStatus::Unspecified) => println!("Status: Unspecified"),
                Err(_) => println!("Status: Unknown ({})", inner.status),
            }
            
            if !inner.proof_uri.is_empty() {
                println!("Proof URI: {}", inner.proof_uri);
            }
            if !inner.error_message.is_empty() {
                println!("Error Message: {}", inner.error_message);
            }
            
            println!("\nFull response:");
            println!("{:#?}", inner);
        }
        Err(e) => {
            eprintln!("Error querying proof status: {}", e);
            eprintln!("Error details: {:#?}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}

