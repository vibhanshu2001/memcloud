use clap::{Parser, Subcommand};
use memsdk::MemCloudClient;
use std::time::Instant;
use std::fs;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

fn get_memcloud_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    home.join(".memcloud")
}

fn get_pid_file() -> PathBuf {
    get_memcloud_dir().join("memnode.pid")
}

fn read_pid() -> Option<i32> {
    let pid_file = get_pid_file();
    if pid_file.exists() {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            return content.trim().parse().ok();
        }
    }
    None
}

fn is_process_running(pid: i32) -> bool {
    // Check if process exists by sending signal 0
    signal::kill(Pid::from_raw(pid), None).is_ok()
}

#[derive(Parser)]
#[command(author, version, about = "MemCloud CLI - Manage your distributed in-memory data store", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "/tmp/memcloud.sock")]
    socket: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the MemCloud node daemon
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
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
    /// Check the version of memcli and the connected node
    Version,
}

#[derive(Subcommand)]
enum NodeAction {
    /// Start the MemCloud node daemon in background
    Start {
        /// Name for this node (visible to peers)
        #[arg(long, short, default_value = "MyNode")]
        name: String,
        /// Port for peer-to-peer communication
        #[arg(long, short, default_value_t = 8080)]
        port: u16,
    },
    /// Stop the running MemCloud node daemon
    Stop,
    /// Check if the node daemon is running
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Node { action } => {
            handle_node_action(action)?;
        }
        other => {
            // All other commands require connecting to the daemon
            let mut client = MemCloudClient::connect().await?;
            handle_data_command(other, &mut client).await?;
        }
    }

    Ok(())
}

fn handle_node_action(action: NodeAction) -> anyhow::Result<()> {
    let memcloud_dir = get_memcloud_dir();
    let pid_file = get_pid_file();

    match action {
        NodeAction::Start { name, port } => {
            // Check if already running
            if let Some(pid) = read_pid() {
                if is_process_running(pid) {
                    println!("⚠️  MemCloud node is already running (PID: {})", pid);
                    return Ok(());
                }
            }

            // Create directory if needed
            fs::create_dir_all(&memcloud_dir)?;

            // Spawn memnode as a detached background process
            println!("🚀 Starting MemCloud node '{}' on port {}...", name, port);
            
            let child = Command::new("memnode")
                .args(["--name", &name, "--port", &port.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;

            let pid = child.id();
            fs::write(&pid_file, pid.to_string())?;

            println!("✅ Node started successfully (PID: {})", pid);
            println!("\n   Use 'memcli node status' to check the node.");
            println!("   Use 'memcli node stop' to stop the node.");
        }
        NodeAction::Stop => {
            if let Some(pid) = read_pid() {
                if is_process_running(pid) {
                    println!("🛑 Stopping MemCloud node (PID: {})...", pid);
                    signal::kill(Pid::from_raw(pid), Signal::SIGTERM)?;
                    let _ = fs::remove_file(&pid_file);
                    println!("✅ Node stopped.");
                } else {
                    println!("⚠️  Node is not running (stale PID file found).");
                    let _ = fs::remove_file(&pid_file);
                }
            } else {
                println!("⚠️  No MemCloud node is running.");
            }
        }
        NodeAction::Status => {
            if let Some(pid) = read_pid() {
                if is_process_running(pid) {
                    println!("✅ MemCloud node is running (PID: {})", pid);
                } else {
                    println!("❌ MemCloud node is not running (stale PID file).");
                    let _ = fs::remove_file(&pid_file);
                }
            } else {
                println!("❌ MemCloud node is not running.");
            }
        }
    }
    Ok(())
}

async fn handle_data_command(cmd: Commands, client: &mut MemCloudClient) -> anyhow::Result<()> {
    match cmd {
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
        Commands::Node { .. } => unreachable!(), // Handled above
        Commands::Version => {
            println!("memcli {}", env!("CARGO_PKG_VERSION"));
            // Try to connect to node to get its version?
            // Currently RPC doesn't have a version method.
            // For now, simple client version is enough.
        }
    }
    Ok(())
}

fn target_peer_string(peer: Option<String>) -> Option<String> {
    peer
}
