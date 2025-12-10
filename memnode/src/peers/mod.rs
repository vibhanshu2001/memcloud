use uuid::Uuid;
use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::net::TcpStream;
use crate::net::Message;
use crate::blocks::BlockManager;
use log::{info, error, warn};
use anyhow::Result;
use serde::{Serialize, Deserialize};

use tokio::io::BufWriter;
use crate::net::auth::{Identity, handshake_initiator};
use crate::net::secure_stream::SecureWriter;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    #[allow(dead_code)]
    pub id: Uuid,
    pub addr: SocketAddr,
    pub name: String,
    pub total_memory: u64,
    pub used_memory: u64,
    pub ram_quota: u64,
    pub remote_used_storage: u64,
    pub connection: Option<Arc<tokio::sync::Mutex<SecureWriter>>>, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerMetadata {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub total_memory: u64,
    pub used_memory: u64,
}

pub struct PeerManager {
    peers: Arc<DashMap<Uuid, PeerInfo>>,
    pending_requests: Arc<DashMap<crate::metadata::BlockId, tokio::sync::broadcast::Sender<Vec<u8>>>>,
    pending_key_requests: Arc<DashMap<String, tokio::sync::broadcast::Sender<Vec<u8>>>>,
    self_id: Uuid,
    self_name: String,
    identity: Arc<Identity>,
}

impl PeerManager {
    pub fn new(self_id: Uuid, self_name: String) -> Self {
        let identity = Arc::new(Identity::new(self_id, self_name.clone()));
        Self {
            peers: Arc::new(DashMap::new()),
            pending_requests: Arc::new(DashMap::new()),
            pending_key_requests: Arc::new(DashMap::new()),
            self_id,
            self_name,
            identity, // Store identity for handshakes
        }
    }

    pub fn get_identity(&self) -> Arc<Identity> {
        self.identity.clone()
    }
    
    pub async fn add_discovered_peer(&self, id: Uuid, addr: SocketAddr, block_manager: Arc<crate::blocks::InMemoryBlockManager>, peer_manager: Arc<PeerManager>, ram_quota: u64) -> Result<()> {
        if self.peers.contains_key(&id) {
            return Ok(());
        }

        // Check if we are already connected to this address (avoid duplicates)
        for entry in self.peers.iter() {
            if entry.value().addr == addr {
                info!("Already connected to peer at {}", addr);
                return Ok(());
            }
        }

        info!("Connecting to peer {} at {}", id, addr);
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                info!("Connected TCP to {}, starting handshake...", id);
                
                match handshake_initiator(&mut stream, &self.identity, ram_quota).await {
                    Ok(session) => {
                        info!("Handshake success with {}. Negotiated encryption.", session.peer_name);
                        
                        let (reader, writer) = stream.into_split();
                        
                        use crate::net::secure_stream::{SecureReader, SecureWriter};
                        let secure_reader = SecureReader::new(reader, &session.recv_key);
                        let secure_writer = SecureWriter::from_raw(writer, &session.send_key);
                        
                        let writer_arc = Arc::new(tokio::sync::Mutex::new(secure_writer));

                        let peer_info = PeerInfo {
                            id,
                            addr,
                            name: session.peer_name,
                            total_memory: 0, 
                            used_memory: 0,
                            ram_quota, 
                            remote_used_storage: 0,
                            connection: Some(writer_arc.clone()),
                        };
                        
                        self.peers.insert(id, peer_info);
                        
                        use crate::net::handle_connection_split;
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection_split(secure_reader, writer_arc, addr, id, block_manager, peer_manager).await {
                                error!("Connection error (outgoing) to {}: {}", addr, e);
                            }
                        });
                        
                    }
                    Err(e) => {
                         error!("Handshake failed with {}: {}", addr, e);
                         return Err(e);
                    }
                }
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
                // Serialize message
                let data = bincode::serialize(msg)?;
                // Send Frame
                writer.send_frame(&data).await?;
                return Ok(());
            }
        }
        anyhow::bail!("Peer not connected")
    }

    pub fn get_peer_metadata_list(&self) -> Vec<PeerMetadata> {
        self.peers.iter()
            .map(|kv| PeerMetadata {
                id: kv.key().to_string(),
                name: kv.value().name.clone(),
                addr: kv.value().addr.to_string(),
                total_memory: kv.value().total_memory,
                used_memory: kv.value().used_memory,
            })
            .collect()
    }
    
    pub fn list_peers(&self) -> Vec<String> {
        self.peers.iter()
            .map(|kv| format!("{} ({}) @ {}", kv.key(), kv.value().name, kv.value().addr))
            .collect()
    }

    pub async fn manual_connect(&self, addr_str: &str, block_manager: Arc<crate::blocks::InMemoryBlockManager>, peer_manager: Arc<PeerManager>, ram_quota: u64) -> Result<()> {
        let addr: SocketAddr = addr_str.parse()?;
        let id_placeholder = Uuid::new_v4(); 
        self.add_discovered_peer(id_placeholder, addr, block_manager, peer_manager, ram_quota).await
    }
    
    // Call from TransportServer after accepting an incoming authenticated connection
    pub fn register_authenticated_peer(&self, id: Uuid, addr: SocketAddr, name: String, connection: Arc<tokio::sync::Mutex<SecureWriter>>, quota: u64) {
         let info = PeerInfo {
             id, 
             addr,
             name,
              total_memory: 0,
              used_memory: 0,
              ram_quota: quota, 
              remote_used_storage: 0,
              connection: Some(connection)
         };
         self.peers.insert(id, info);
    }

    pub fn disconnect_peer(&self, peer_id: Uuid) -> bool {
        if self.peers.remove(&peer_id).is_some() {
            info!("Disconnected peer {}", peer_id);
            true
        } else {
            warn!("Attempted to disconnect unknown peer {}", peer_id);
            false
        }
    }

    pub fn try_reserve_storage(&self, peer_id: Uuid, size: u64) -> bool {
        if let Some(mut peer) = self.peers.get_mut(&peer_id) {
            if peer.remote_used_storage + size <= peer.ram_quota {
                peer.remote_used_storage += size;
                return true;
            } else {
                warn!("Peer {} quota exceeded. Used: {}, Requested: {}, Limit: {}", peer_id, peer.remote_used_storage, size, peer.ram_quota);
                return false;
            }
        }
        false
    }

    pub fn update_peer_ram_quota(&self, peer_id: Uuid, remote_quota: u64) {
         info!("Peer {} updated their quota for us to {} bytes", peer_id, remote_quota);
    }

    pub async fn set_allowed_quota(&self, peer_id: Uuid, new_quota: u64) -> Result<()> {
        if let Some(mut peer) = self.peers.get_mut(&peer_id) {
            info!("Updating allowed quota for peer {} to {} bytes", peer_id, new_quota);
            peer.ram_quota = new_quota;
            
            // Notify peer
            if let Some(conn) = &peer.connection {
                let mut writer = conn.lock().await;
                let msg = Message::UpdateQuota { quota: new_quota };
                let data = bincode::serialize(&msg)?;
                writer.send_frame(&data).await?;
            }
            Ok(())
        } else {
             anyhow::bail!("Peer not found")
        }
    }

    pub fn release_storage(&self, peer_id: Uuid, size: u64) {
        if let Some(mut peer) = self.peers.get_mut(&peer_id) {
            if peer.remote_used_storage >= size {
                 peer.remote_used_storage -= size;
            } else {
                peer.remote_used_storage = 0;
            }
        }
    }
    
    pub async fn request_block(&self, peer_id: Uuid, block_id: crate::metadata::BlockId) -> Result<()> {
        let msg = Message::GetBlock { id: block_id };
        self.send_to_peer(peer_id, &msg).await
    }

    pub async fn wait_for_block(&self, block_id: crate::metadata::BlockId) -> Result<Vec<u8>> {
        let tx = self.pending_requests.entry(block_id).or_insert_with(|| {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx
        }).clone();

        let mut rx = tx.subscribe();
        
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
        let mut connections = Vec::new();
        for item in self.peers.iter() {
            if let Some(conn) = &item.value().connection {
                connections.push(conn.clone());
            }
        }

        for conn in connections {
            let mut w = conn.lock().await;
            // Serialize
            let data = bincode::serialize(&msg)?;
            let _ = w.send_frame(&data).await;
        }
        Ok(())
    }

    pub async fn wait_for_key(&self, key: &str) -> Result<Vec<u8>> {
        let tx = self.pending_key_requests.entry(key.to_string()).or_insert_with(|| {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx
        }).clone();

        let mut rx = tx.subscribe();
        
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
