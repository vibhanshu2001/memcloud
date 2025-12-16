pub mod c_api;

use serde::{Serialize, Deserialize};
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(windows)]
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;


pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Ok(0);
    }
    
    let (digits, suffix) = s.split_at(s.find(|c: char| !c.is_numeric()).unwrap_or(s.len()));
    let val: u64 = digits.parse().map_err(|_| anyhow::anyhow!("Invalid number provided"))?;
    
    match suffix.trim() {
        "b" | "" => Ok(val),
        "kb" | "k" => Ok(val * 1024),
        "mb" | "m" => Ok(val * 1024 * 1024),
        "gb" | "g" => Ok(val * 1024 * 1024 * 1024),
        "tb" | "t" => Ok(val * 1024 * 1024 * 1024 * 1024),
        _ => anyhow::bail!("Invalid size suffix: {}", suffix),
    }
}

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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Pinned,
    Cache,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum SdkCommand {
    Store { #[serde(with = "serde_bytes")] data: Vec<u8>, durability: Option<Durability> },
    StoreRemote { #[serde(with = "serde_bytes")] data: Vec<u8>, target: Option<String>, durability: Option<Durability> },
    Load { #[serde(with = "string_id")] id: BlockId },
    Free { #[serde(with = "string_id")] id: BlockId },
    ListPeers,
    Connect { addr: String, quota: Option<u64> },
    UpdatePeerQuota { peer_id: String, quota: u64 },
    Disconnect { peer_id: String },
    Set { key: String, #[serde(with = "serde_bytes")] data: Vec<u8>, target: Option<String>, durability: Option<Durability> },
    Get { key: String, target: Option<String> },
    ListKeys { pattern: String },
    Stat,
    PollConnection { addr: String },
    StreamStart { size_hint: Option<u64> },
    StreamChunk { stream_id: u64, chunk_seq: u32, #[serde(with = "serde_bytes")] data: Vec<u8> },
    StreamFinish { stream_id: u64, target: Option<String>, durability: Option<Durability> },
    Flush { target: Option<String> },
    // Trust & Consent
    TrustList,
    TrustRemove { key_or_name: String },
    ConsentList,
    ConsentApprove { session_id: String, trust_always: bool },
    ConsentDeny { session_id: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerMetadata {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub total_memory: u64,
    pub used_memory: u64,
    pub quota: u64,
    pub allowed_quota: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustedDevice {
    pub public_key: String,
    pub name: String,
    pub first_seen: u64,
    pub last_approved: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PendingConsent {
    pub session_id: String,
    pub peer_pubkey: String,
    pub peer_name: String,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "res")]
pub enum SdkResponse {
    Stored { #[serde(with = "string_id")] id: BlockId },
    Loaded { #[serde(with = "serde_bytes")] data: Vec<u8> },
    Success,
    List { items: Vec<String> },
    PeerList { peers: Vec<PeerMetadata> },
    PeerConnected { metadata: PeerMetadata },
    Error { msg: String },
    Status { blocks: usize, peers: usize, memory_usage: usize },
    StreamStarted { stream_id: u64 },
    FlushSuccess,
    TrustedList { items: Vec<TrustedDevice> },
    ConsentList { items: Vec<PendingConsent> },
    ConnectionStatus { state: String, msg: Option<String> },
}

#[cfg(unix)]
type InnerStream = UnixStream;
#[cfg(windows)]
type InnerStream = TcpStream;

pub struct MemCloudClient {
    stream: InnerStream,
}

impl MemCloudClient {
    #[cfg(unix)]
    pub async fn connect() -> Result<Self> {
        let stream = UnixStream::connect("/tmp/memcloud.sock").await?;
        Ok(Self { stream })
    }

    #[cfg(windows)]
    pub async fn connect() -> Result<Self> {
        let stream = TcpStream::connect("127.0.0.1:7070").await?;
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

    pub async fn store(&mut self, data: &[u8], durability: Durability) -> Result<BlockId> {
        let cmd = SdkCommand::Store { data: data.to_vec(), durability: Some(durability) };
        match self.send_command(cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn store_remote(&mut self, data: &[u8], target: Option<String>, durability: Durability) -> Result<BlockId> {
        let cmd = SdkCommand::StoreRemote { data: data.to_vec(), target, durability: Some(durability) };
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

    pub async fn connect_peer(&mut self, addr: &str, quota: Option<u64>) -> Result<(String, Option<String>)> {
         let cmd = SdkCommand::Connect { addr: addr.to_string(), quota };
         match self.send_command(cmd).await? {
            SdkResponse::ConnectionStatus { state, msg } => Ok((state, msg)),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response to Connect"),
        }
    }
    
    pub async fn poll_connection(&mut self, addr: &str) -> Result<(String, Option<String>)> {
         let cmd = SdkCommand::PollConnection { addr: addr.to_string() };
         match self.send_command(cmd).await? {
            SdkResponse::ConnectionStatus { state, msg } => Ok((state, msg)),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response to PollConnection"),
        }
    }
    
    pub async fn disconnect_peer(&mut self, peer_id: &str) -> Result<()> {
        let cmd = SdkCommand::Disconnect { peer_id: peer_id.to_string() };
        match self.send_command(cmd).await? {
             SdkResponse::Success => Ok(()),
             SdkResponse::Error { msg } => anyhow::bail!(msg),
             _ => anyhow::bail!("Unexpected response to Disconnect"),
        }
    }

    pub async fn update_peer_quota(&mut self, peer_id: &str, quota: u64) -> Result<()> {
        let cmd = SdkCommand::UpdatePeerQuota { peer_id: peer_id.to_string(), quota };
        match self.send_command(cmd).await? {
           SdkResponse::Success => Ok(()),
           SdkResponse::Error { msg } => anyhow::bail!(msg),
           _ => anyhow::bail!("Unexpected response"),
       }
   }
    
    // KV Methods
    pub async fn set(&mut self, key: &str, data: &[u8], target: Option<String>, durability: Durability) -> Result<BlockId> {
         let cmd = SdkCommand::Set { key: key.to_string(), data: data.to_vec(), target, durability: Some(durability) };
         match self.send_command(cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    
    pub async fn get(&mut self, key: &str, target: Option<String>) -> Result<Vec<u8>> {
        let cmd = SdkCommand::Get { key: key.to_string(), target };
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

    pub async fn stream_data<R>(&mut self, mut source: R, size_hint: Option<u64>, target: Option<String>) -> Result<BlockId> 
    where R: tokio::io::AsyncRead + Unpin 
    {
        // 1. Start
        let start_cmd = SdkCommand::StreamStart { size_hint };
        let stream_id = match self.send_command(start_cmd).await? {
            SdkResponse::StreamStarted { stream_id } => stream_id,
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response to StreamStart"),
        };

        // 2. Chunks
        let mut buffer = vec![0u8; 1024 * 64]; // 64KB chunks
        let mut seq = 0;
        loop {
            let n = source.read(&mut buffer).await?;
            if n == 0 { break; }
            
            let chunk_cmd = SdkCommand::StreamChunk {
                stream_id,
                chunk_seq: seq,
                data: buffer[..n].to_vec(),
            
            };
            
            match self.send_command(chunk_cmd).await? {
                SdkResponse::Success => {},
                SdkResponse::Error { msg } => anyhow::bail!(msg),
                _ => anyhow::bail!("Unexpected response to StreamChunk"),
            }
            seq += 1;
        }

        // 3. Finish
        let finish_cmd = SdkCommand::StreamFinish { stream_id, target, durability: None };
        match self.send_command(finish_cmd).await? {
            SdkResponse::Stored { id } => Ok(id),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response to StreamFinish"),
        }
    }
    // Trust API
    pub async fn list_trusted(&mut self) -> Result<Vec<TrustedDevice>> {
        let cmd = SdkCommand::TrustList;
        match self.send_command(cmd).await? {
            SdkResponse::TrustedList { items } => Ok(items),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn remove_trusted(&mut self, key_or_name: &str) -> Result<()> {
        let cmd = SdkCommand::TrustRemove { key_or_name: key_or_name.to_string() };
        match self.send_command(cmd).await? {
            SdkResponse::Success => Ok(()),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn list_consent(&mut self) -> Result<Vec<PendingConsent>> {
        let cmd = SdkCommand::ConsentList;
        match self.send_command(cmd).await? {
            SdkResponse::ConsentList { items } => Ok(items),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn approve_consent(&mut self, session_id: &str, trust_always: bool) -> Result<()> {
        let cmd = SdkCommand::ConsentApprove { session_id: session_id.to_string(), trust_always };
        match self.send_command(cmd).await? {
            SdkResponse::Success => Ok(()),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }

    pub async fn deny_consent(&mut self, session_id: &str) -> Result<()> {
        let cmd = SdkCommand::ConsentDeny { session_id: session_id.to_string() };
        match self.send_command(cmd).await? {
            SdkResponse::Success => Ok(()),
            SdkResponse::Error { msg } => anyhow::bail!(msg),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100").unwrap(), 100);
        assert_eq!(parse_size("1b").unwrap(), 1);
        assert_eq!(parse_size("1kb").unwrap(), 1024);
        assert_eq!(parse_size("1 kb").unwrap(), 1024);
        assert_eq!(parse_size("1 MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("1.5gb").is_err(), true);
        assert_eq!(parse_size("512MB").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_size("0").unwrap(), 0);
    }
}
