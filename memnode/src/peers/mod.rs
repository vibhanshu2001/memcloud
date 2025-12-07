use uuid::Uuid;
use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use crate::net::{Message, send_message};
use log::{info, error};
use anyhow::Result;
use tokio::net::tcp::OwnedWriteHalf;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: Uuid,
    pub addr: SocketAddr,
    pub name: String,
    // We keep a connection guarded by Mutex for thread safety
    // In a high-perf system, we might use message passing (channels) instead of Mutex<TcpStream>
    pub connection: Option<Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>>, 
}

pub struct PeerManager {
    peers: Arc<DashMap<Uuid, PeerInfo>>,
    pending_requests: Arc<DashMap<crate::metadata::BlockId, tokio::sync::broadcast::Sender<Vec<u8>>>>,
    pending_key_requests: Arc<DashMap<String, tokio::sync::broadcast::Sender<Vec<u8>>>>,
    self_id: Uuid,
    self_name: String,
}

impl PeerManager {
    pub fn new(self_id: Uuid, self_name: String) -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            pending_requests: Arc::new(DashMap::new()),
            pending_key_requests: Arc::new(DashMap::new()),
            self_id,
            self_name,
        }
    }

    // We use stored self_id/name to send Hello
    
    pub async fn add_discovered_peer(&self, id: Uuid, addr: SocketAddr, block_manager: Arc<crate::blocks::InMemoryBlockManager>, peer_manager: Arc<PeerManager>) -> Result<()> {
        if self.peers.contains_key(&id) {
            return Ok(());
        }

        info!("Connecting to peer {} at {}", id, addr);
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                info!("Connected to peer {}", id);
                
                use tokio::io::AsyncWriteExt;
                // Send Hello
                let hello = Message::Hello {
                    version: 1,
                    node_id: self.self_id,
                    name: self.self_name.clone(),
                };
                
                // Split
                let (reader, mut writer) = stream.into_split();
                
                // Manual send hello on writer
                let bytes = bincode::serialize(&hello)?;
                let len = bytes.len() as u32;
                writer.write_all(&len.to_be_bytes()).await?;
                writer.write_all(&bytes).await?;

                let writer_arc = Arc::new(tokio::sync::Mutex::new(writer));

                let peer_info = PeerInfo {
                    id,
                    addr,
                    name: "Unknown".to_string(), 
                    connection: Some(writer_arc.clone()),
                };
                
                self.peers.insert(id, peer_info);
                
                // Spawn Read Loop
                use crate::net::handle_connection_split;
                tokio::spawn(async move {
                    if let Err(e) = handle_connection_split(reader, writer_arc, addr, block_manager, peer_manager).await {
                        error!("Connection error (outgoing) to {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to connect to peer {}: {}", id, e);
            }
        }
        Ok(())
    }

    pub fn get_peer_id_by_name(&self, name_query: &str) -> Option<Uuid> {
        for item in self.peers.iter() {
            if item.value().name.contains(name_query) || item.value().name == name_query {
                return Some(*item.key());
            }
        }
        None
    }


    pub async fn get_available_peer(&self) -> Option<Uuid> {
        for item in self.peers.iter() {
            if item.value().connection.is_some() {
                return Some(*item.key());
            }
        }
        None
    }

    pub async fn send_to_peer(&self, peer_id: Uuid, msg: &Message) -> Result<()> {
        if let Some(peer) = self.peers.get(&peer_id) {
            if let Some(conn) = &peer.connection {
                let mut writer = conn.lock().await;
                use crate::net::send_message_locked;
                send_message_locked(&mut writer, msg).await?;
                return Ok(());
            }
        }
        anyhow::bail!("Peer not connected")
    }

    pub fn list_peers(&self) -> Vec<String> {
        self.peers.iter()
            .map(|kv| format!("{} ({}) @ {}", kv.key(), kv.value().name, kv.value().addr))
            .collect()
    }

    pub async fn manual_connect(&self, addr_str: &str, block_manager: Arc<crate::blocks::InMemoryBlockManager>, peer_manager: Arc<PeerManager>) -> Result<()> {
        let addr: SocketAddr = addr_str.parse()?;
        let id = Uuid::new_v4(); // We don't know ID yet, we assume they accept us.
        // Actually, if we use UUID v4, we might duplicate? 
        // Ideally we should do a handshake to get their real ID.
        // But for add_discovered_peer, we usually know ID from mDNS.
        // Here we just use random ID. When we get their Hello, we might need to update ID in map?
        // Updating key in DashMap is hard. We might need to remove and re-insert.
        // For now, let's proceed.
        self.add_discovered_peer(id, addr, block_manager, peer_manager).await
    }

    pub fn handle_hello(&self, real_id: Uuid, name: String, addr: SocketAddr, connection: Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>) {
        // 1. Check if we already know this stable ID
        if let Some(mut peer) = self.peers.get_mut(&real_id) {
             info!("Received Hello from existing peer {}. Updating name to '{}'", real_id, name);
             peer.name = name;
             // Update connection if the new one is different/fresher? 
             // For now, if we are receiving Hello on a connection, that connection is active.
             // It might be the SAME connection arc if we just spawned it.
             peer.connection = Some(connection);
             return;
        }
    
        // 2. Check if we have a temporary entry for this Addr (Manual Connect case)
        // (Scan all peers... suboptimal but fine for MVP)
        let mut temp_id: Option<Uuid> = None;
        
        // We can't iterate and remove effectively with DashMap easily in one pass if we need the key.
        // DashMap iter gives references.
        for entry in self.peers.iter() {
            if entry.value().addr.ip() == addr.ip() && entry.value().addr.port() == addr.port() {
                // Found a match by address!
                temp_id = Some(*entry.key());
                break;
            }
        }
    
        if let Some(old_id) = temp_id {
            // Remove old
            if let Some((_, _old_info)) = self.peers.remove(&old_id) {
                info!("Handshake: Upgrading peer {} to real ID {} ('{}')", old_id, real_id, name);
            }
        } else {
            info!("Handshake: Registering new incoming peer {} ('{}') from {}", real_id, name, addr);
        }
        
        // 3. Insert new / authenticated entry
        let new_info = PeerInfo {
            id: real_id,
            addr,
            name,
            connection: Some(connection),
        };
        self.peers.insert(real_id, new_info);
    }
    // Ideally we need a request-response correlation ID or a channel to wait for the specific response.
    // For MVP, since we don't have a complex transport with message IDs yet, 
    // we might need to implement a 'send_and_wait' style or just fire and forget for now (but we need data back).
    // Let's implement a simple `fetch_block` that opens a NEW short-lived connection just for fetching 
    // OR we need to upgrade our TransportServer to handle responses.
    
    // DECISION: To keep it simple and robust without refactoring the whole Transport into an actor model:
    // We will open a transient connection to fetch the block if the persistent one is busy or doesn't support request/reply easily.
    // actually, we have a persistent connection. We can send GetBlock. 
    // But how do we get the reply back to *this* caller?
    
    // REFACTORING PLAN:
    // 1. We need `BlockManager` to be able to "ask" for a block.
    // 2. The `Transport` receives `BlockData`. It needs to notify `BlockManager` or a pending waiter.
    // 
    // COMPROMISE FOR MVP SPEED:
    // We will use a "RPC-like" blocking call on the existing stream? No, multiple async calls might mix.
    // Let's use a `DashMap<BlockId, tokio::sync::oneshot::Sender<Vec<u8>>>` in PeerManager/network layer 
    // to track pending requests!
    
    // We'll need to update PeerManager struct first.
    // For this step, I'll just add the method signature.
    pub async fn request_block(&self, peer_id: Uuid, block_id: crate::metadata::BlockId) -> Result<()> {
        let msg = Message::GetBlock { id: block_id };
        self.send_to_peer(peer_id, &msg).await
    }

    // Call this to start waiting, then call request_block
    pub async fn wait_for_block(&self, block_id: crate::metadata::BlockId) -> Result<Vec<u8>> {
        // Create a channel if not exists
        let tx = self.pending_requests.entry(block_id).or_insert_with(|| {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx
        }).clone();

        let mut rx = tx.subscribe();
        
        // Wait for data (timeout 5s)
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(e)) => anyhow::bail!("Recv error: {}", e),
            Err(_) => anyhow::bail!("Timeout waiting for block data"),
        }
    }

    pub fn satisfy_request(&self, block_id: crate::metadata::BlockId, data: Vec<u8>) {
        if let Some(tx) = self.pending_requests.get(&block_id) {
            let _ = tx.send(data);
        }
    }

    pub async fn broadcast_get_key(&self, key: &str) -> Result<()> {
        let msg = Message::GetKey { key: key.to_string() };
        // Clean iteration to avoid holding lock across await
        let mut connections = Vec::new();
        for item in self.peers.iter() {
            if let Some(conn) = &item.value().connection {
                connections.push(conn.clone());
            }
        }

        for conn in connections {
            let mut w = conn.lock().await;
            use crate::net::send_message_locked;
            // Ignore errors on broadcast
            let _ = send_message_locked(&mut w, &msg).await;
        }
        Ok(())
    }

    pub async fn wait_for_key(&self, key: &str) -> Result<Vec<u8>> {
        let tx = self.pending_key_requests.entry(key.to_string()).or_insert_with(|| {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx
        }).clone();

        let mut rx = tx.subscribe();
        
        // Wait shorter time for keys? Or same 5s?
        // If multiple peers have it, we take first.
        match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(e)) => anyhow::bail!("Recv error: {}", e),
            Err(_) => anyhow::bail!("Timeout waiting for key"),
        }
    }

    pub fn satisfy_key_request(&self, key: &str, data: Vec<u8>) {
        if let Some(tx) = self.pending_key_requests.get(key) {
             let _ = tx.send(data);
        }
    }
    pub fn get_self_id(&self) -> Uuid {
        self.self_id
    }
    
    pub fn get_self_name(&self) -> String {
        self.self_name.clone()
    }
}
