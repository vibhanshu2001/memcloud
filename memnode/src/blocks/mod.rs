use anyhow::Result;
use crate::metadata::BlockId;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use log::info;
use crate::peers::PeerManager;
use crate::net::Message;

#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
pub trait BlockManager: Send + Sync {
    fn put_block(&self, block: Block) -> Result<()>;
    fn get_block(&self, id: BlockId) -> Result<Option<Block>>;
    fn evict_block(&self, id: BlockId) -> Result<Option<Block>>;
    fn free_space(&self) -> u64;
    fn used_space(&self) -> u64;
}

#[derive(Clone)]
pub struct InMemoryBlockManager {
    pub(crate) blocks: Arc<DashMap<BlockId, Block>>,
    key_index: Arc<DashMap<String, BlockId>>,
    peer_manager: Arc<PeerManager>,
    // Map to track if a block ID is stored remotely to route GETs
    remote_locations: Arc<DashMap<BlockId, uuid::Uuid>>,
    // Track total memory usage in bytes
    current_memory: Arc<AtomicU64>,
    max_memory: u64,
    // Streaming partial uploads
    active_uploads: Arc<DashMap<u64, Vec<u8>>>,
}

impl InMemoryBlockManager {
    pub fn new(peer_manager: Arc<PeerManager>, max_memory: u64) -> Self {
        Self {
            blocks: Arc::new(DashMap::new()),
            key_index: Arc::new(DashMap::new()),
            peer_manager,
            remote_locations: Arc::new(DashMap::new()),
            current_memory: Arc::new(AtomicU64::new(0)),
            max_memory,
            active_uploads: Arc::new(DashMap::new()),
        }
    }

    // New explicit method for remote storage (for demo/policy)
    // In a real system, put_block would decide automatically
    pub async fn put_block_remote(&self, block: Block, target: Option<String>) -> Result<()> {
         // Find a peer
         let peer_id = if let Some(t) = target {
             // Try to parse as UUID first
             if let Ok(uid) = uuid::Uuid::parse_str(&t) {
                 Some(uid)
             } else {
                 // Try name
                 self.peer_manager.get_peer_id_by_name(&t)
             }
         } else {
             self.peer_manager.get_available_peer().await
         };

         if let Some(peer_id) = peer_id {
             info!("Offloading block {} to peer {}", block.id, peer_id);
             
             let msg = Message::PutBlock {
                 id: block.id,
                 data: block.data,
             };
             
             // Send
             self.peer_manager.send_to_peer(peer_id, &msg).await?;
             
             // Record location
             self.remote_locations.insert(block.id, peer_id);
             Ok(())
         } else {
             anyhow::bail!("No suitable peer found for remote storage");
         }
    }

    pub fn get_peer_list(&self) -> Vec<String> {
        self.peer_manager.list_peers()
    }
    
    pub fn get_peer_ext_list(&self) -> Vec<crate::peers::PeerMetadata> {
        self.peer_manager.get_peer_metadata_list()
    }

    pub async fn connect_peer(&self, addr: &str, block_manager: Arc<InMemoryBlockManager>, quota: u64) -> Result<crate::peers::PeerMetadata> {
        self.peer_manager.manual_connect(addr, block_manager, self.peer_manager.clone(), quota).await
    }
    
    pub async fn disconnect_peer(&self, target: &str) -> Result<bool> {
         let peer_id = if let Ok(uid) = uuid::Uuid::parse_str(target) {
              Some(uid)
         } else {
              self.peer_manager.get_peer_id_by_name(target)
         };
         
         if let Some(id) = peer_id {
             Ok(self.peer_manager.disconnect_peer(id).await)
         } else {
             Ok(false)
         }
    }

    pub async fn update_peer_quota(&self, target: &str, quota: u64) -> Result<()> {
        let peer_id = if let Ok(uid) = uuid::Uuid::parse_str(target) {
             Some(uid)
        } else {
             self.peer_manager.get_peer_id_by_name(target)
        };

        if let Some(id) = peer_id {
             self.peer_manager.set_allowed_quota(id, quota).await
        } else {
             anyhow::bail!("Peer '{}' not found", target)
        }
    }

    pub fn put_named_block(&self, key: String, block: Block) -> Result<()> {
        let id = block.id;
        self.put_block(block)?;
        self.key_index.insert(key.clone(), id);
        info!("Stored named block '{}' -> {}", key, id);
        Ok(())
    }
    
    pub fn get_named_block_id(&self, key: &str) -> Option<BlockId> {
        self.key_index.get(key).map(|v| *v)
    }

    pub async fn get_distributed_key(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // 1. Try Local
        if let Some(id) = self.get_named_block_id(key) {
            if let Ok(Some(block)) = self.get_block_async(id).await {
                return Ok(Some(block.data));
            }
        }
        
        // 2. Try Remote Broadcast
        // info!("Key '{}' not found locally, broadcasting query...", key);
        
        // Start waiting
        let fut = self.peer_manager.wait_for_key(key);
        
        // Broadcast
        self.peer_manager.broadcast_get_key(key).await?;
        
        // Wait
        match fut.await {
            Ok(data) => {
                info!("Found key '{}' on a peer!", key);
                Ok(Some(data))
            }
            Err(_) => {
                Ok(None)
            }
        }
    }

    pub fn list_keys(&self, pattern: &str) -> Vec<String> {
        let starts_wild = pattern.starts_with('*');
        let ends_wild = pattern.ends_with('*');
        let clean_pat = pattern.trim_matches('*');
        
        // Optimize: Special case for "*" to just collect directly
        if pattern == "*" {
            return self.key_index.iter().map(|kv| kv.key().clone()).collect();
        }

        self.key_index.iter()
            .filter(|kv| {
                let k = kv.key();
                if starts_wild && ends_wild {
                    k.contains(clean_pat)
                } else if starts_wild {
                    k.ends_with(clean_pat)
                } else if ends_wild {
                    k.starts_with(clean_pat)
                } else {
                    k == clean_pat
                }
            })
            .map(|kv| kv.key().clone())
            .collect()
    }

    pub async fn get_block_async(&self, id: BlockId) -> Result<Option<Block>> {
         // 1. Try Local
         if let Some(entry) = self.blocks.get(&id) {
            return Ok(Some(entry.clone()));
         }
         
         // 2. Check Remote
         if let Some(peer_id) = self.remote_locations.get(&id) {
             info!("Block {} is remote at {}, fetching...", id, peer_id.value());
             
             // A. Start Waiting
             let fut = self.peer_manager.wait_for_block(id);
             
             // B. Send Request
             self.peer_manager.request_block(*peer_id.value(), id).await?;
             
             // C. Wait Result
             let data = fut.await?;
             info!("Fetched block {} from peer", id);
             return Ok(Some(Block { id, data }));
         }
         
         Ok(None)
    }

    // Streaming Logic
    pub fn start_stream(&self, size_hint: Option<u64>) -> u64 {
        let stream_id = rand::random::<u64>();
        let capacity = size_hint.unwrap_or(0) as usize;
        self.active_uploads.insert(stream_id, Vec::with_capacity(capacity));
        info!("Started stream upload ID: {} (Hint: {:?})", stream_id, size_hint);
        stream_id
    }

    pub fn append_stream(&self, stream_id: u64, data: Vec<u8>) -> Result<()> {
        if let Some(mut stream_buffer) = self.active_uploads.get_mut(&stream_id) {
            stream_buffer.extend_from_slice(&data);
            Ok(())
        } else {
            anyhow::bail!("Stream ID {} not found or already closed", stream_id);
        }
    }

    pub fn finalize_stream(&self, stream_id: u64) -> Result<Vec<u8>> {
        if let Some((_, data)) = self.active_uploads.remove(&stream_id) {
            Ok(data)
        } else {
            anyhow::bail!("Stream ID {} not found", stream_id);
        }
    }

    pub fn flush(&self) {
        self.blocks.clear();
        self.key_index.clear();
        self.remote_locations.clear();
        self.active_uploads.clear();
        self.current_memory.store(0, Ordering::Relaxed);
        info!("Cluster memory flushed locally.");
    }

    pub async fn flush_remote(&self, target: String) -> Result<()> {
        let peer_id = if let Ok(uid) = uuid::Uuid::parse_str(&target) {
             Some(uid)
        } else {
             self.peer_manager.get_peer_id_by_name(&target)
        };

        if let Some(id) = peer_id {
            info!("Sending Flush command to peer {}", id);
            let msg = Message::Flush;
            self.peer_manager.send_to_peer(id, &msg).await?;
            Ok(())
        } else {
            anyhow::bail!("Peer '{}' not found", target);
        }
    }

    pub fn get_max_memory(&self) -> u64 {
        self.max_memory
    }
}

// We need async for remote, but the trait is synchronous for now.
// For the MVP, we might compromise or specificy async trait.
// Since we want to update the trait, let's stick to local usage for trait compliance
// and handle remote via specific cast or async channel in a better design.
// BUT for this task, I will just impl the standard put/get to be local, 
// and add specific logic for the DEMO to use the remote path.

impl BlockManager for InMemoryBlockManager {
    fn put_block(&self, block: Block) -> Result<()> {
        let size = block.data.len();
        self.blocks.insert(block.id, block.clone());
        self.current_memory.fetch_add(size as u64, Ordering::Relaxed);
        info!("Stored block {} ({} bytes)", block.id, size);
        Ok(())
    }

    fn get_block(&self, id: BlockId) -> Result<Option<Block>> {
        if let Some(entry) = self.blocks.get(&id) {
            Ok(Some(entry.clone()))
        } else {
            // Check remote? (Stub for now, requires async Get)
            if self.remote_locations.contains_key(&id) {
                 info!("Block {} is remote (fetching not implemented in sync get_block)", id);
            }
            Ok(None)
        }
    }

    fn evict_block(&self, id: BlockId) -> Result<Option<Block>> {
        if let Some((_, block)) = self.blocks.remove(&id) {
            let size = block.data.len() as u64;
            self.current_memory.fetch_sub(size, Ordering::Relaxed);
            info!("Evicted block {}", id);
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    fn free_space(&self) -> u64 {
        u64::MAX // Unlimited for now
    }

    fn used_space(&self) -> u64 {
        self.current_memory.load(Ordering::Relaxed)
    }
}
