use std::{fs, error::Error, time::Duration};
use serde::Deserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer, read_keypair_file},
    system_instruction,
    transaction::Transaction,
};
use yellowstone_grpc_client::{GeyserGrpcClient, ClientTlsConfig};
use tonic::transport::Channel;
use futures::StreamExt;
use log::{info, error};

#[derive(Debug, Deserialize)]
struct Config {
    grpc_endpoint: String,
    x_token: String,
    rpc_url: String,
    from_keypair: String,
    to_address: String,
    amount_sol: f64,
}

async fn send_sol(
    rpc: &RpcClient,
    from: &Keypair,
    to: &Pubkey,
    lamports: u64,
) -> Result<String, Box<dyn Error>> {
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let instruction = system_instruction::transfer(&from.pubkey(), to, lamports);
    let tx = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&from.pubkey()),
        &[from],
        recent_blockhash,
    );
    let signature = rpc.send_and_confirm_transaction(&tx).await?;
    Ok(signature.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let config_str = fs::read_to_string("config.yaml")?;
    let config: Config = serde_yaml::from_str(&config_str)?;

    let rpc = RpcClient::new(config.rpc_url.clone());
    let from = read_keypair_file(&config.from_keypair)?;
    let to: Pubkey = config.to_address.parse()?;
    let lamports = (config.amount_sol * 1_000_000_000.0) as u64;

    let tls_config = ClientTlsConfig::new();
    let channel = Channel::from_shared(config.grpc_endpoint.clone())?
        .tls_config(tls_config)?
        .connect()
        .await?;
    let mut client = GeyserGrpcClient::new(channel, Some(config.x_token.clone()));

    let mut stream = client.subscribe_blocks().await?;

    info!("Subscribed to block stream. Waiting for new blocks...");

    while let Some(block) = stream.next().await {
        match block {
            Ok(block_info) => {
                info!("New block detected: slot {}", block_info.slot);
                match send_sol(&rpc, &from, &to, lamports).await {
                    Ok(sig) => info!("Transaction sent: {}", sig),
                    Err(e) => error!("Failed to send transaction: {}", e),
                }
            }
            Err(e) => error!("Error receiving block: {}", e),
        }
    }

    Ok(())
}
