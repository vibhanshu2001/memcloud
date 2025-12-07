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
}

impl InMemoryBlockManager {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self {
            blocks: Arc::new(DashMap::new()),
            key_index: Arc::new(DashMap::new()),
            peer_manager,
            remote_locations: Arc::new(DashMap::new()),
            current_memory: Arc::new(AtomicU64::new(0)),
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

    pub async fn connect_peer(&self, addr: &str, block_manager: Arc<InMemoryBlockManager>) -> Result<()> {
        self.peer_manager.manual_connect(addr, block_manager, self.peer_manager.clone()).await
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
