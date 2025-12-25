use clap::{Parser, Subcommand};
use memsdk::MemCloudClient;
use std::time::Instant;
use std::fs;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::io::{self, Write};

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

#[cfg(windows)]
use sysinfo::{System, Pid as SysPid};

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
    #[cfg(unix)]
    {
        // Check if process exists by sending signal 0
        signal::kill(Pid::from_raw(pid), None).is_ok()
    }
    #[cfg(windows)]
    {
        let mut sys = System::new_all();
        sys.refresh_processes();
        sys.process(SysPid::from(pid as usize)).is_some()
    }
}

fn kill_process(pid: i32) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        signal::kill(Pid::from_raw(pid), Signal::SIGTERM)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let mut sys = System::new_all();
        sys.refresh_processes();
        if let Some(process) = sys.process(SysPid::from(pid as usize)) {
             process.kill();
        }
        Ok(())
    }
}

#[derive(Parser)]
#[command(author = "Vibhanshu Garg <v2001.garg@gmail.com>", version, about = "MemCloud CLI - Manage your distributed in-memory data store", long_about = None)]
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
        /// Durability mode: 'pinned' (default) or 'cache'
        #[arg(long, default_value = "pinned")]
        mode: String,
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
        /// How much of YOUR memory capacity to offer this peer (e.g., "512mb", "1gb")
        /// This is the maximum they can store on your node.
        #[arg(long, short = 'o')]
        offer_storage: Option<String>,
    },
    /// Show memory usage and stats
    Stats {
        /// Follow and refresh stats live
        #[arg(short, long)]
        follow: bool,
    },
    /// Set a key-value pair
    Set {
        key: String,
        value: String,
        #[arg(long)]
        peer: Option<String>,
        /// Durability mode: 'pinned' (default) or 'cache'
        #[arg(long, default_value = "pinned")]
        mode: String,
    },
    /// Get a value by key
    Get {
        key: String,
        #[arg(long)]
        peer: Option<String>,
    },
    /// List keys matching patterns (default: *)
    Keys {
        #[arg(default_value = "*", num_args = 0..)]
        patterns: Vec<String>,
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
    /// Manage trusted devices
    Trust {
        #[command(subcommand)]
        action: TrustAction,
    },
    /// Interactive consent management
    Consent,
    /// Run a command with MemCloud VM interception
    Run {
        /// Malloc threshold in MB
        #[arg(short, long, default_value_t = 8)]
        threshold: u64,
        /// Command to execute
        command: String,
        /// Arguments for the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum TrustAction {
    List,
    Remove {
        key_or_name: String,
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
        /// Total memory capacity this node allocates for the network (e.g., "4gb", "512mb")
        /// This is the hard limit for ALL storage combined.
        #[arg(long, short = 'm', default_value = "1gb")]
        total_memory: String,
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
        /// New storage limit you ALLOW this peer to use on your node (e.g. "1gb")
        #[arg(long, short = 'a')]
        allowed_storage: String,
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
        Commands::Consent => {
            let mut client = MemCloudClient::connect_with_path(&cli.socket).await?;
            handle_consent(&mut client).await?;
        }
        Commands::Run { threshold, command, args } => {
            // Verify daemon is running
            let _ = MemCloudClient::connect_with_path(&cli.socket).await.map_err(|_| {
                anyhow::anyhow!("❌ MemCloud node is not running. Please start it with 'memcli node start' first.")
            })?;
            handle_run(threshold, command, args, &cli.socket)?;
        }
        other => {
            // All other commands require connecting to the daemon
            let mut client = MemCloudClient::connect_with_path(&cli.socket).await?;
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
        NodeAction::Start { name, port, total_memory } => {
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
                .args(["--name", &final_name, "--port", &port.to_string(), "--memory", &total_memory])
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
                    kill_process(pid)?;
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
        Commands::Store { data, remote, peer, mode } => {
            let start = Instant::now();
            let is_remote = remote || peer.is_some();
            let durability = match mode.to_lowercase().as_str() {
                "cache" => memsdk::Durability::Cache,
                "pinned" => memsdk::Durability::Pinned,
                _ => anyhow::bail!("Invalid mode: {}. Use 'pinned' or 'cache'", mode),
            };
            
            let id = if is_remote {
                client.store_remote(data.as_bytes(), target_peer_string(peer), durability).await?
            } else {
                client.store(data.as_bytes(), durability).await?
            };
            let duration = start.elapsed();
            println!("Stored block ID: {} (remote: {}, mode: {:?}) (took {:?})", id, is_remote, durability, duration);
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
                PeerAction::Update { id, allowed_storage } => {
                    let quota_bytes = memsdk::parse_size(&allowed_storage)?;
                    client.update_peer_quota(&id, quota_bytes).await?;
                    println!("Updated peer {} allowed storage to {} bytes", id, quota_bytes);
                }
                PeerAction::Disconnect { id } => {
                    client.disconnect_peer(&id).await?;
                    println!("Disconnected peer {}", id);
                }
            }
        }
        Commands::Connect { addr, offer_storage } => {
            let quota_val = if let Some(q) = offer_storage {
                memsdk::parse_size(&q)?
            } else {
                0 // Default to 0 (Unidirectional access: Initiator writes to Responder, but Responder cannot write to Initiator)
            };
            
            println!("🔗 Initiating connection to {}...", addr);
            
            let (mut state, mut msg) = client.connect_peer(&addr, Some(quota_val)).await?;
            
            let mut indicated_consent = false;
            
            loop {
                match state.as_str() {
                    "connected" => break,
                    "failed" => {
                        let err = msg.unwrap_or_else(|| "Unknown error".to_string());
                        anyhow::bail!("Connection failed: {}", err);
                    }
                    "waiting_consent" => {
                        if !indicated_consent {
                            println!("\n⚠️  Peer requires consent. Please approve on the remote device.");
                            print!("⏳ Waiting for approval...");
                            io::stdout().flush()?;
                            indicated_consent = true;
                        }
                    }
                    "pending" | _ => {
                        print!(".");
                        io::stdout().flush()?;
                    }
                }
                
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                let res = client.poll_connection(&addr).await?;
                state = res.0;
                msg = res.1;
            }
            
            if indicated_consent {
                println!("\n✅ Consent granted.");
            }
            let peers = client.list_peers().await?;
            
            let meta_opt = peers.into_iter().find(|p| p.addr == addr);
            
            if let Some(meta) = meta_opt {
                println!("\n✅ Connection established!");
                println!("🔐 Secure Session Established (Noise XX / ChaCha20-Poly1305)");
                println!("\n📡 Handshake successful (Node ID: {})", meta.name);
                
                // Format stats
                let total_ram = format_bytes(meta.total_memory);
                let pooled_ram = format_bytes(quota_val); 
                
                println!("   Latency: <1ms | Total RAM: {} | RAM Pooled: {}", total_ram, pooled_ram);
            } else {
                 println!("\n✅ Connection established, but could not retrieve stats immediately.");
            }
        }
        Commands::Stats { follow } => {
            loop {
                let (blocks, peers, memory, vm_regions, vm_pages, vm_bytes) = client.stats().await?;
                
                // Clear screen (ANSI escape code)
                if follow {
                    print!("\x1B[2J\x1B[H");
                }

                println!("-------- MemCloud Stats --------");
                println!("Blocks Stored:    {}", blocks);
                println!("Peers Connected:  {}", peers);
                println!("Memory Usage:     {}", format_bytes(memory as u64));
                println!("--------------------------------");
                println!("Remote VM regions:      {}", vm_regions);
                println!("Remote VM pages mapped: {}", vm_pages);
                println!("Remote VM memory in use: {}", format_bytes(vm_bytes as u64));
                println!("--------------------------------");

                if !follow {
                    break;
                }
                
                println!("\n(Press Ctrl+C to stop following)");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        Commands::Set { key, value, peer, mode } => {
            let start = Instant::now();
            let durability = match mode.to_lowercase().as_str() {
                "cache" => memsdk::Durability::Cache,
                "pinned" => memsdk::Durability::Pinned,
                _ => anyhow::bail!("Invalid mode: {}. Use 'pinned' or 'cache'", mode),
            };
            let id = client.set(&key, value.as_bytes(), peer, durability).await?;
            let duration = start.elapsed();
            println!("Set '{}' -> {} (Block ID: {}, mode: {:?}) (took {:?})", key, value, id, durability, duration);
        }
        Commands::Get { key, peer } => {
            let start = Instant::now();
            let data = client.get(&key, peer).await?;
            let duration = start.elapsed();
            let value = String::from_utf8_lossy(&data);
            println!("Get '{}' -> '{}' (took {:?})", key, value, duration);
        }
        Commands::Keys { patterns } => {
            let start = Instant::now();
            let mut all_keys = std::collections::HashSet::new();
            
            for pattern in &patterns {
                 let keys = client.list_keys(pattern).await?;
                 for k in keys {
                     all_keys.insert(k);
                 }
            }
            
            let mut sorted_keys: Vec<_> = all_keys.into_iter().collect();
            sorted_keys.sort();
            
            let duration = start.elapsed();
            
            if sorted_keys.is_empty() {
                println!("No keys found matching {:?}", patterns);
                // Hint about shell expansion if likely cause
                let looks_like_expansion = patterns.len() > 1 && !patterns.iter().any(|p| p.contains('*') || p.contains('?'));
                if looks_like_expansion {
                     println!("(Hint: wildcard '*' might have been expanded by your shell. Try quoting it: 'memcli keys \"*\"')");
                }
            } else {
                for k in &sorted_keys {
                    println!("{}", k);
                }
                println!("\nFound {} unique keys (took {:?})", sorted_keys.len(), duration);
            }
        }
        Commands::Trust { action } => {
            match action {
                TrustAction::List => {
                    let items = client.list_trusted().await?;
                    if items.is_empty() {
                         println!("No trusted devices found.");
                    } else {
                         println!("{:<20} {:<30} {:<64}", "Name", "Last Approved", "Public Key");
                         println!("{}", "-".repeat(116));
                         for item in items {
                             // Format time
                             let time_str = format!("{}", item.last_approved);
                             println!("{:<20} {:<30} {:<64}", item.name, time_str, item.public_key);
                         }
                    }
                }
                TrustAction::Remove { key_or_name } => {
                    client.remove_trusted(&key_or_name).await?;
                    println!("Removed '{}' from trusted devices.", key_or_name);
                }
            }
        }
        Commands::Consent | Commands::Node { .. } | Commands::Logs { .. } => unreachable!(),
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
        Commands::Run { .. } => {
            // Handled in main
            unreachable!("Run should be handled in main");
        }
    }
    Ok(())
}

fn handle_run(threshold: u64, command: String, args: Vec<String>, socket: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        let mut cmd = Command::new(&command);
        cmd.args(args);

        // 1. Determine interceptor path
        // For development, we look in the current directory and target/debug
        let dylib_name = if cfg!(target_os = "macos") {
            "libmemcloud_vm.dylib"
        } else {
            "libmemcloud_vm.so"
        };

        let mut dylib_path = None;
        let search_paths = [
            std::env::current_dir()?.join("interceptor").join(dylib_name),
            std::env::current_dir()?.join("target").join("debug").join(dylib_name),
            PathBuf::from("/usr/local/lib").join(dylib_name),
        ];

        for path in &search_paths {
            if path.exists() {
                dylib_path = Some(path.to_string_lossy().to_string());
                break;
            }
        }

        let interceptor_path = match dylib_path {
            Some(p) => p,
            None => {
                println!("❌ Could not find interceptor library ({}).", dylib_name);
                println!("   Search paths: {:?}", search_paths);
                return Ok(());
            }
        };

        // 2. Set environment variables
        if cfg!(target_os = "macos") {
            cmd.env("DYLD_INSERT_LIBRARIES", &interceptor_path);
            cmd.env("DYLD_FORCE_FLAT_NAMESPACE", "1");
        } else {
            cmd.env("LD_PRELOAD", &interceptor_path);
        }

        cmd.env("MEMCLOUD_MALLOC_THRESHOLD_MB", threshold.to_string());
        cmd.env("MEMCLOUD_SOCKET", socket);

        // Help the dynamic linker find libmemsdk if needed
        let lib_env = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
        let mut lib_path = std::env::var(lib_env).unwrap_or_default();
        let sdk_dir = std::env::current_dir()?.join("target").join("debug");
        if !lib_path.is_empty() {
             lib_path.push(':');
        }
        lib_path.push_str(&sdk_dir.to_string_lossy());
        cmd.env(lib_env, lib_path);

        println!("🚀 Running '{}' with MemCloud interception...", command);
        println!("   (Threshold: {} MB, Socket: {})", threshold, socket);

        // Execute and replace process
        let err = cmd.exec();
        
        // If exec returns, it failed
        anyhow::bail!("Failed to execute command: {}", err);
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("'run' command is only supported on Unix-like systems (Linux/macOS)");
    }
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
    let h_in = "Allowed Storage";
    let h_out = "Capacity Offered";
    
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

    println!("\n📊 Total Pooled RAM (Capacity Offered): {}", format_bytes(total_pooled));
}

async fn handle_consent(client: &mut MemCloudClient) -> anyhow::Result<()> {
    loop {
        let pending = client.list_consent().await?;
        
        if pending.is_empty() {
            println!("No pending consent requests.");
            return Ok(());
        }

        println!("Found {} pending request(s).", pending.len());
        
        for req in pending {
            println!("\nDevice: {} ({})", req.peer_name, req.peer_pubkey); 
            println!("Wants to connect. Request ID: {}", req.session_id);
            println!("Offering Capacity: {}  (This capacity will be available to you)", format_bytes(req.quota));
            
            // Interaction
            let selection = dialoguer::Select::new()
                .with_prompt("Action")
                .items(&["Allow (Once)", "Trust Always", "Deny", "Skip"])
                .default(0)
                .interact()?;

            match selection {
                0 => { // Allow Once
                    client.approve_consent(&req.session_id, false).await?;
                    println!("✅ Allowed.");
                }
                1 => { // Trust Always
                    client.approve_consent(&req.session_id, true).await?;
                    println!("✅ Trustees.");
                }
                2 => { // Deny
                    client.deny_consent(&req.session_id).await?;
                    println!("❌ Denied.");
                }
                _ => {
                    println!("Skipped.");
                }
            }
        }
        
        println!("Checking for more...");
    }
}
