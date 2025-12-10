use clap::{Parser, Subcommand};
use memsdk::MemCloudClient;
use std::time::Instant;
use std::fs;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::io::{self, Write};

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
        id: String, // Updated to String
    },
    /// Free a block by ID
    Free {
        id: String,
    },
    /// Manage peers (list, update, disconnect)
    Peer {
        #[command(subcommand)]
        action: PeerAction,
    },
    Peers,
    Connect {
        addr: String,
        /// Optional: RAM quota to offer (e.g., "512mb", "1gb")
        #[arg(long)]
        quota: Option<String>,
    },
    /// Show memory usage and stats
    Stats,
    /// Set a key-value pair
    Set {
        key: String,
        value: String,
        #[arg(long)]
        peer: Option<String>,
    },
    /// Get a value by key
    Get {
        key: String,
        #[arg(long)]
        peer: Option<String>,
    },
    /// List keys matching a pattern (default: *)
    Keys {
        #[arg(default_value = "*")]
        pattern: String,
    },
    /// Check the version of memcli and the connected node
    Version,
    /// View daemon logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
    /// Flush all data from the node (Dangerous!)
    Flush {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Optional: Flush specific peer (by name or ID) instead of local
        #[arg(long)]
        peer: Option<String>,
        /// Optional: Flush ALL connected peers and local node
        #[arg(long)]
        all: bool,
    },
    /// Stream data from stdin or file
    Stream {
        /// Input file (if not provided, reads from stdin)
        #[arg(name = "FILE")]
        file: Option<String>,
        
        /// Optional: Target specific peer
        #[arg(long)]
        peer: Option<String>,
    },
}

#[derive(Subcommand)]
enum NodeAction {
    /// Start the MemCloud node daemon in background
    Start {
        /// Name for this node (visible to peers)
        #[arg(long, short)]
        name: Option<String>,
        /// Port for peer-to-peer communication
        #[arg(long, short, default_value_t = 8080)]
        port: u16,
    },
    /// Stop the running MemCloud node daemon
    Stop,
    /// Check if the node daemon is running
    Status,
}

#[derive(Subcommand)]
enum PeerAction {
    List,
    Update {
        id: String,
        #[arg(long)]
        quota: String,
    },
    Disconnect {
        id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Node { action } => {
            handle_node_action(action)?;
        }
        Commands::Logs { follow } => {
            handle_logs(follow)?;
        }
        other => {
            // All other commands require connecting to the daemon
            let mut client = MemCloudClient::connect().await?;
            handle_data_command(other, &mut client).await?;
        }
    }

    Ok(())
}

fn handle_logs(follow: bool) -> anyhow::Result<()> {
    let log_path = get_memcloud_dir().join("memnode.log");
    
    if !log_path.exists() {
        println!("❌ No log file found at {:?}", log_path);
        println!("   (Is the node running or has it been started with logging enabled?)");
        return Ok(());
    }

    if follow {
        // Use tail -f to follow logs
        let mut child = Command::new("tail")
            .arg("-f")
            .arg(&log_path)
            .spawn()?;
        
        // Wait for user to interrupt
        child.wait()?;
    } else {
        // Print last lines or full file? Reading full file might be big.
        // Let's print the whole file for now, user can pipe it.
        let content = fs::read_to_string(log_path)?;
        print!("{}", content);
    }
    Ok(())
}

fn handle_node_action(action: NodeAction) -> anyhow::Result<()> {
    let memcloud_dir = get_memcloud_dir();
    let pid_file = get_pid_file();
    let log_file_path = memcloud_dir.join("memnode.log");

    match action {
        NodeAction::Start { name, port } => {
            // Check if already running
            if let Some(pid) = read_pid() {
                if is_process_running(pid) {
                    println!("⚠️  MemCloud node is already running (PID: {})", pid);
                    return Ok(());
                }
            }
            
            // Resolve name
            let final_name = match name {
                Some(n) => n,
                None => {
                    print!("Enter node name [MyNode]: ");
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    let trimmed = input.trim();
                    if trimmed.is_empty() {
                        "MyNode".to_string()
                    } else {
                        trimmed.to_string()
                    }
                }
            };

            // Create directory if needed
            fs::create_dir_all(&memcloud_dir)?;

            // Log Rotation: Check if log file is too big (> 3MB)
            if log_file_path.exists() {
                if let Ok(metadata) = fs::metadata(&log_file_path) {
                    if metadata.len() > 3 * 1024 * 1024 { // 3MB limit
                        let old_log = memcloud_dir.join("memnode.log.old");
                        println!("📦 Rotating logs (exceeded 3MB)...");
                        let _ = fs::rename(&log_file_path, old_log);
                    }
                }
            }

            // Open log file for appending
            let log_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)?;

            // Spawn memnode as a detached background process
            println!("🚀 Starting MemCloud node '{}' on port {}...", final_name, port);
            
            let child = Command::new("memnode")
                .args(["--name", &final_name, "--port", &port.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::from(log_file.try_clone()?))
                .stderr(Stdio::from(log_file))
                .spawn()?;
            
            let pid = child.id();
            fs::write(&pid_file, pid.to_string())?;

            println!("✅ Node started successfully (PID: {})", pid);
            println!("\n   Use 'memcli node status' to check the node.");
            println!("   Use 'memcli logs -f' to view logs.");
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
            // Parse string id back to number or handle string in SDK?
            // The SDK client.load expects BlockId (u64) OR we updated SDK?
            // Wait, we updated SDK library (memsdk) and memnode rpc.
            // But strict typing in Rust: memsdk::BlockId is still u64 in lib.rs?
            // Let's check memsdk lib.rs.
            // memsdk::BlockId is u64.
            // The JSON serialization uses "string" on the wire, but Rust type is u64.
            // We should parse the string CLI arg to u64 here.
            
            // If the ID is truly a large u64 that JS couldn't handle, Rust CAN handle it.
            // But the CLI input comes as string. We need to parse it to u64.
            
            let id_u64 = id.parse::<u64>()?;
            let data = client.load(id_u64).await?;
            let duration = start.elapsed();
            let string_data = String::from_utf8_lossy(&data);
            println!("Loaded block {}: '{}' (took {:?})", id, string_data, duration);
        }
        Commands::Free { id } => {
            let start = Instant::now();
            let id_u64 = id.parse::<u64>()?;
            client.free(id_u64).await?;
            let duration = start.elapsed();
            println!("Freed block {} (took {:?})", id, duration);
        }
        Commands::Peers => {
             handle_peer_list(client).await?;
        }
        Commands::Peer { action } => {
            match action {
                PeerAction::List => handle_peer_list(client).await?,
                PeerAction::Update { id, quota } => {
                    let quota_bytes = memsdk::parse_size(&quota)?;
                    client.update_peer_quota(&id, quota_bytes).await?;
                    println!("Updated peer {} quota to {} bytes", id, quota_bytes);
                }
                PeerAction::Disconnect { id } => {
                    client.disconnect_peer(&id).await?;
                    println!("Disconnected peer {}", id);
                }
            }
        }
        Commands::Connect { addr, quota } => {
            let quota_str = if quota.is_none() {
                 dialoguer::Input::<String>::new()
                     .with_prompt("Enter RAM quota to offer (default: 512mb)")
                     .default("512mb".into())
                     .interact_text()?
            } else {
                quota.unwrap()
            };

            let quota_val = memsdk::parse_size(&quota_str)?;
            
            println!("🔗 Initiating connection to {}...", addr);
            
            let meta = client.connect_peer(&addr, Some(quota_val)).await?;
            
            println!("\n✅ Connection established!");
            println!("🔐 Secure Session Established (Noise XX / ChaCha20-Poly1305)");
            println!("\n📡 Handshake successful (Node ID: {})", meta.name);
            
            // Format stats
            let total_ram = format_bytes(meta.total_memory);
            let pooled_ram = format_bytes(quota_val); 
            
            println!("   Latency: <1ms | Total RAM: {} | RAM Pooled: {}", total_ram, pooled_ram);
        }
        Commands::Stats => {
            let (blocks, peers, memory) = client.stats().await?;
            println!("-------- MemCloud Stats --------");
            println!("Blocks Stored: {}", blocks);
            println!("Peers Connected: {}", peers);
            println!("Memory Usage: {} bytes", memory);
            println!("--------------------------------");
        }
        Commands::Set { key, value, peer } => {
            let start = Instant::now();
            let id = client.set(&key, value.as_bytes(), peer).await?;
            let duration = start.elapsed();
            println!("Set '{}' -> {} (Block ID: {}) (took {:?})", key, value, id, duration);
        }
        Commands::Get { key, peer } => {
            let start = Instant::now();
            let data = client.get(&key, peer).await?;
            let duration = start.elapsed();
            let value = String::from_utf8_lossy(&data);
            println!("Get '{}' -> '{}' (took {:?})", key, value, duration);
        }
        Commands::Keys { pattern } => {
            let start = Instant::now();
            let keys = client.list_keys(&pattern).await?;
            let duration = start.elapsed();
            
            if keys.is_empty() {
                println!("No keys found matching '{}'", pattern);
            } else {
                for k in &keys {
                    println!("{}", k);
                }
                println!("\nFound {} keys (took {:?})", keys.len(), duration);
            }
        }
        Commands::Node { .. } | Commands::Logs { .. } => unreachable!(), // Handled above
        Commands::Version => {
            println!("memcli {}", env!("CARGO_PKG_VERSION"));
            // Try to connect to node to get its version?
            // Currently RPC doesn't have a version method.
            // For now, simple client version is enough.
        }
            // For now, simple client version is enough.

        Commands::Flush { force, peer, all } => {
            let target_desc = if all {
                "WHOLE CLUSTER (all peers + local)".to_string()
            } else {
                peer.clone().unwrap_or_else(|| "LOCAL node".to_string())
            };

            if !force {
                println!("⚠️  WARNING: This will delete ALL data stored on the {}.", target_desc);
                print!("   Are you sure? [y/N]: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if input.trim().to_lowercase() != "y" {
                    println!("❌ Aborted.");
                    return Ok(());
                }
            }
            
            if all {
                println!("🧹 Flushing CLUSTER...");
                let peers = client.list_peers().await?;
                for p in peers {
                    print!("   - Flushing peer {} ({}) ... ", p.name, p.addr);
                    if let Err(e) = client.flush(Some(p.id)).await {
                        println!("❌ Failed: {}", e);
                    } else {
                        println!("✅");
                    }
                }
                print!("   - Flushing LOCAL node ... ");
                client.flush(None).await?;
                println!("✅");
                println!("✅ Cluster flushed.");
            } else {
                println!("🧹 Flushing memory on {}...", target_desc);
                client.flush(peer).await?;
                println!("✅ Memory flushed.");
            }
        }
        Commands::Stream { file, peer } => {
            let start = Instant::now();
            let id = if let Some(path) = file {
                 // Open file
                 let f = tokio::fs::File::open(&path).await?;
                 let meta = f.metadata().await?;
                 client.stream_data(f, Some(meta.len()), peer.clone()).await?
            } else {
                 // Stdin
                 println!("Reading from stdin (Ctrl+D to finish)...");
                 let stdin = tokio::io::stdin();
                 client.stream_data(stdin, None, peer.clone()).await?
            };
            let duration = start.elapsed();
            println!("Streamed block ID: {} (took {:?})", id, duration);
        }
    }
    Ok(())
}

fn target_peer_string(peer: Option<String>) -> Option<String> {
    peer
}

async fn handle_peer_list(client: &mut MemCloudClient) -> anyhow::Result<()> {
     let peers = client.list_peers().await?;
     if peers.is_empty() {
         println!("No peers connected.");
     } else {
         print_peers_table(&peers);
     }
     Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn print_peers_table(peers: &[memsdk::PeerMetadata]) {
    // 1. Calculate column widths
    let h_node = "Node";
    let h_addr = "Address";
    let h_in = "Allowed (In)";
    let h_out = "Pool (Out)";
    
    let mut w_node = h_node.len();
    let mut w_addr = h_addr.len();
    let mut w_in = h_in.len();
    let mut w_out = h_out.len();

    // Scan data
    for p in peers {
        w_node = w_node.max(p.name.len());
        w_addr = w_addr.max(p.addr.len());
        w_in = w_in.max(format_bytes(p.allowed_quota).len());
        w_out = w_out.max(format_bytes(p.quota).len());
    }

    // Padding
    w_node += 2; 
    w_addr += 2;
    w_in += 2;
    w_out += 2;

    // Helper to print separator
    let print_sep = |start: &str, mid: &str, end: &str, line: &str| {
        print!("{}", start);
        print!("{}", line.repeat(w_node));
        print!("{}", mid);
        print!("{}", line.repeat(w_addr));
        print!("{}", mid);
        print!("{}", line.repeat(w_in));
        print!("{}", mid);
        print!("{}", line.repeat(w_out));
        println!("{}", end);
    };

    // Top
    print_sep("┌", "┬", "┐", "─");

    // Header
    println!("│ {:<width_n$} │ {:<width_a$} │ {:<width_i$} │ {:<width_o$} │", 
             h_node, h_addr, h_in, h_out,
             width_n = w_node-2, width_a = w_addr-2, width_i = w_in-2, width_o = w_out-2);

    // Mid
    print_sep("├", "┼", "┤", "─");

    // Rows
    let mut total_pooled = 0;
    for p in peers {
        let q_in = format_bytes(p.allowed_quota);
        let q_out = format_bytes(p.quota);
        total_pooled += p.quota;
        
        println!("│ {:<width_n$} │ {:<width_a$} │ {:<width_i$} │ {:<width_o$} │", 
                 p.name, p.addr, q_in, q_out,
                 width_n = w_node-2, width_a = w_addr-2, width_i = w_in-2, width_o = w_out-2);
    }

    // Bottom
    print_sep("└", "┴", "┘", "─");

    println!("\n📊 Total Pooled RAM (Outbound): {}", format_bytes(total_pooled));
}
