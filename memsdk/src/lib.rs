pub mod c_api;

use serde::{Serialize, Deserialize};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;


pub type BlockId = u64;

// Helper for string serialization
mod string_id {
    use serde::{Deserialize, Deserializer, Serializer};
    use super::BlockId;

    pub fn serialize<S>(id: &BlockId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&id.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum SdkCommand {
    Store { #[serde(with = "serde_bytes")] data: Vec<u8> },
    StoreRemote { #[serde(with = "serde_bytes")] data: Vec<u8>, target: Option<String> },
    Load { #[serde(with = "string_id")] id: BlockId },
    Free { #[serde(with = "string_id")] id: BlockId },
    ListPeers,
    Connect { addr: String },
    Set { key: String, #[serde(with = "serde_bytes")] data: Vec<u8> },
    Get { key: String },
    ListKeys { pattern: String },
    Stat,
    StreamStart { size_hint: Option<u64> },
    StreamChunk { stream_id: u64, chunk_seq: u32, #[serde(with = "serde_bytes")] data: Vec<u8> },
    StreamFinish { stream_id: u64 },
    Flush { target: Option<String> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerMetadata {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub total_memory: u64,
    pub used_memory: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "res")]
pub enum SdkResponse {
    Stored { #[serde(with = "string_id")] id: BlockId },
    Loaded { #[serde(with = "serde_bytes")] data: Vec<u8> },
    Success,
    List { items: Vec<String> },
    PeerList { peers: Vec<PeerMetadata> },
    Error { msg: String },
    Status { blocks: usize, peers: usize, memory_usage: usize },
    StreamStarted { stream_id: u64 },
    FlushSuccess,
}

pub struct MemCloudClient {
    stream: UnixStream,
}

impl MemCloudClient {
    pub async fn connect() -> Result<Self> {
        let stream = UnixStream::connect("/tmp/memcloud.sock").await?;
        Ok(Self { stream })
    }

    async fn send_command(&mut self, cmd: SdkCommand) -> Result<SdkResponse> {
        // Serialize
        let bytes = rmp_serde::to_vec_named(&cmd)?;
        let len = bytes.len() as u32;

        // Send
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&bytes).await?;

        // Receive Response
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        let mut resp_buf = vec![0u8; resp_len];
        self.stream.read_exact(&mut resp_buf).await?;

        // Deserialize
        let resp: SdkResponse = rmp_serde::from_slice(&resp_buf)?;
        Ok(resp)
    }

    pub async fn store(&mut self, data: &[u8]) -> Result<BlockId> {
        let cmd = SdkCommand::Store { data: data.to_vec() };
        match self.send_command(cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn store_remote(&mut self, data: &[u8], target: Option<String>) -> Result<BlockId> {
        let cmd = SdkCommand::StoreRemote { data: data.to_vec(), target };
        match self.send_command(cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn load(&mut self, id: BlockId) -> Result<Vec<u8>> {
        let cmd = SdkCommand::Load { id };
        match self.send_command(cmd).await? {
            SdkResponse::Loaded { data } => Ok(data),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    
    pub async fn free(&mut self, id: BlockId) -> Result<()> {
        let cmd = SdkCommand::Free { id };
        match self.send_command(cmd).await? {
            SdkResponse::Success => Ok(()),
             SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    
    pub async fn list_peers(&mut self) -> Result<Vec<PeerMetadata>> {
        let cmd = SdkCommand::ListPeers;
        match self.send_command(cmd).await? {
            SdkResponse::PeerList { peers } => Ok(peers),
            // Fallback for older nodes? No versioning yet.
            SdkResponse::List { items: _items } => {
                // mock convert? or error?
                // Assuming version alignment. 
                // But if we encounter List, it means old node.
                // Converting string list to metadata is hard because we lack stats/id separation in string?
                // String was "ID (Name) @ Addr". We could parse it.
                // Let's iterate and parse if needed, later. For now, assume matching version.
                anyhow::bail!("Received legacy peer list format")
            },
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn connect_peer(&mut self, addr: &str) -> Result<()> {
         let cmd = SdkCommand::Connect { addr: addr.to_string() };
         match self.send_command(cmd).await? {
            SdkResponse::Success => Ok(()),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    
    // KV Methods
    pub async fn set(&mut self, key: &str, data: &[u8]) -> Result<BlockId> {
         let cmd = SdkCommand::Set { key: key.to_string(), data: data.to_vec() };
         match self.send_command(cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    
    pub async fn get(&mut self, key: &str) -> Result<Vec<u8>> {
        let cmd = SdkCommand::Get { key: key.to_string() };
        match self.send_command(cmd).await? {
            SdkResponse::Loaded { data } => Ok(data),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn list_keys(&mut self, pattern: &str) -> Result<Vec<String>> {
        let cmd = SdkCommand::ListKeys { pattern: pattern.to_string() };
        match self.send_command(cmd).await? {
            SdkResponse::List { items } => Ok(items),
             SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn stats(&mut self) -> Result<(usize, usize, usize)> {
        let cmd = SdkCommand::Stat;
        match self.send_command(cmd).await? {
            SdkResponse::Status { blocks, peers, memory_usage } => Ok((blocks, peers, memory_usage)),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn flush(&mut self, target: Option<String>) -> Result<()> {
        let cmd = SdkCommand::Flush { target };
        match self.send_command(cmd).await? {
            SdkResponse::FlushSuccess => Ok(()),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
}
