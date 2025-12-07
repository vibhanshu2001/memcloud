mod blocks;
mod discovery;
mod peers;
mod net;
mod metadata;
mod rpc;

use log::{info, error};
use uuid::Uuid;
use clap::Parser;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    #[arg(short, long, default_value_t = 1024 * 1024 * 1024)] // 1GB default
    memory: u64,

    #[arg(long, default_value = "/tmp/memcloud.sock")]
    socket: String,

    #[arg(long, default_value = "Unnamed Node")]
    name: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logger with mDNS logs suppressed to avoid "No route to host" spam on macOS
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("mdns_sd", log::LevelFilter::Off)
        .init();
    let args = Args::parse();
    let node_id = Uuid::new_v4();

    info!("Starting MemCloud Node {} on port {}", node_id, args.port);

    // 1. Init PeerManager
    let peer_manager = Arc::new(peers::PeerManager::new(node_id, args.name.clone()));

    // 4. Initialize Block Manager
    let block_manager = Arc::new(blocks::InMemoryBlockManager::new(peer_manager.clone()));

    // 3. Start RPC Server
    let rpc_server = rpc::RpcServer::new(&args.socket, block_manager.clone());
    let rpc_handle = tokio::spawn(async move {
        if let Err(e) = rpc_server.run().await {
            error!("RPC Server failed: {}", e);
        }
    });

    // 4. Start Transport Listener
    let transport = net::TransportServer::new(args.port, block_manager.clone(), peer_manager.clone()).await?;
    let transport_port = args.port;

    // 5. Start Discovery (mDNS)
    let discovery = discovery::MdnsDiscovery::new(node_id, args.port, peer_manager.clone(), block_manager.clone())?;
    discovery.start_advertising()?;
    discovery.start_browsing()?;

    // 6. Run Transport Loop
    tokio::select! {
        _ = transport.run() => {},
        _ = rpc_handle => {},
    }

    Ok(())
}
