use clap::{Parser, Subcommand};
use memsdk::MemCloudClient;
use std::time::Instant;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "/tmp/memcloud.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a string as a block
    Store {
        data: String,
        /// Force remote storage
        #[arg(long, short)]
        remote: bool,
        /// Optional: Target specific peer by name or ID
        #[arg(long)]
        peer: Option<String>,
    },
    /// Load a block by ID (as string)
    Load {
        id: u64,
    },
    /// Free a block by ID
    Free {
        id: u64,
    },
    /// List connected peers
    Peers,
    /// Manually connect to a peer (e.g., 192.168.1.5:8080)
    Connect {
        addr: String,
    },
    /// Show memory usage and stats
    Stats,
    /// Set a key-value pair
    Set {
        key: String,
        value: String,
    },
    /// Get a value by key
    Get {
        key: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    // Socket path is now hardcoded in SDK connect for MVP or we should pass it.
    // The previous SDK allowed passing it. The new SDK hardcodes /tmp/memcloud.sock in connect().
    // Ideally we update SDK to accept path.
    // For now, let's ignore the CLI arg or assume default.
    
    let mut client = MemCloudClient::connect().await?;

    match cli.command {
        Commands::Store { data, remote, peer } => {
            let start = Instant::now();
            let is_remote = remote || peer.is_some();
            let id = if is_remote {
                client.store_remote(data.as_bytes(), target_peer_string(peer)).await?
            } else {
                client.store(data.as_bytes()).await?
            };
            let duration = start.elapsed();
            println!("Stored block ID: {} (remote: {}) (took {:?})", id, is_remote, duration);
        }
        Commands::Load { id } => {
            let start = Instant::now();
            let data = client.load(id).await?;
            let duration = start.elapsed();
            let string_data = String::from_utf8_lossy(&data);
            println!("Loaded block {}: '{}' (took {:?})", id, string_data, duration);
        }
        Commands::Free { id } => {
            let start = Instant::now();
            client.free(id).await?;
            let duration = start.elapsed();
            println!("Freed block {} (took {:?})", id, duration);
        }
        Commands::Peers => {
             let peers = client.list_peers().await?;
             if peers.is_empty() {
                 println!("No peers connected.");
             } else {
                 println!("Connected Peers:");
                 for p in peers {
                     println!("- {}", p);
                 }
             }
        }
        Commands::Connect { addr } => {
            client.connect_peer(&addr).await?;
            println!("Connected to {}", addr);
        }
        Commands::Stats => {
            let (blocks, peers, memory) = client.stats().await?;
            println!("-------- MemCloud Stats --------");
            println!("Blocks Stored: {}", blocks);
            println!("Peers Connected: {}", peers);
            println!("Memory Usage: {} bytes", memory);
            println!("Memory Usage: {} bytes", memory);
            println!("--------------------------------");
        }
        Commands::Set { key, value } => {
            let start = Instant::now();
            let id = client.set(&key, value.as_bytes()).await?;
            let duration = start.elapsed();
            println!("Set '{}' -> {} (Block ID: {}) (took {:?})", key, value, id, duration);
        }
        Commands::Get { key } => {
            let start = Instant::now();
            let data = client.get(&key).await?;
            let duration = start.elapsed();
            let value = String::from_utf8_lossy(&data);
            println!("Get '{}' -> '{}' (took {:?})", key, value, duration);
        }
    }

    Ok(())
}

fn target_peer_string(peer: Option<String>) -> Option<String> {
    peer
}
