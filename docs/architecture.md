# MemCloud Architecture

This document describes the internal architecture of MemCloud, including data flows for core operations.

---

## System Overview

```mermaid
flowchart TD

    subgraph AppLayer[Application Layer]
        CLI["MemCLI"]
        SDK["JS / Python / Rust SDK"]
    end

    subgraph LocalDaemon["MemCloud Daemon (Local)"]
        RPC["Local RPC API<br/>(Unix Socket + JSON TCP)"]
        BlockMgr["Block Manager<br/>(Store/Load/Free)"]
        PeerMgr["Peer Manager<br/>(Connections & Routing)"]
        RAM[("Local RAM Cache")]
        Discovery["mDNS Discovery"]
    end

    subgraph RemoteDevice["Remote Device(s)"]
        RemoteDaemon["Remote MemCloud Daemon"]
        RemoteRAM[("Remote RAM Storage")]
    end

    CLI --> RPC
    SDK --> RPC

    RPC --> BlockMgr
    BlockMgr --> RAM
    BlockMgr --> PeerMgr
    PeerMgr --> Discovery

    Discovery --> RemoteDaemon
    PeerMgr <-->|"TCP / Binary Protocol"| RemoteDaemon

    RemoteDaemon --> RemoteRAM
```

---

## Data Flow: STORE Operation

When a client stores data:

```mermaid
sequenceDiagram
    participant Client as SDK/CLI
    participant RPC as RPC Server
    participant BM as Block Manager
    participant RAM as Local RAM
    participant PM as Peer Manager
    participant Remote as Remote Node

    Client->>RPC: Store { data }
    RPC->>BM: store_block(data)
    
    alt Local Storage
        BM->>RAM: Insert block
        RAM-->>BM: Block ID
    else Remote Storage (--remote flag)
        BM->>PM: route_to_peer(data)
        PM->>Remote: StoreBlock { data }
        Remote-->>PM: Block ID
    end
    
    BM-->>RPC: Stored { id }
    RPC-->>Client: Block ID
```

---

## Data Flow: LOAD Operation

When a client loads data:

```mermaid
sequenceDiagram
    participant Client as SDK/CLI
    participant RPC as RPC Server
    participant BM as Block Manager
    participant RAM as Local RAM
    participant PM as Peer Manager
    participant Remote as Remote Node

    Client->>RPC: Load { id }
    RPC->>BM: get_block(id)
    
    alt Block Found Locally
        BM->>RAM: Lookup block
        RAM-->>BM: Block data
    else Block on Remote Peer
        BM->>PM: fetch_from_peer(id)
        PM->>Remote: RequestBlock { id }
        Remote-->>PM: BlockData
        PM-->>BM: Block data
    end
    
    BM-->>RPC: Loaded { data }
    RPC-->>Client: Data
```

---

## Peer Discovery Flow

MemCloud uses mDNS (Multicast DNS) for automatic peer discovery on the local network:

```mermaid
sequenceDiagram
    participant NodeA as Node A (New)
    participant mDNS as mDNS Multicast
    participant NodeB as Node B (Existing)

    NodeA->>mDNS: Advertise "_memcloud._tcp.local."
    NodeA->>mDNS: Browse for "_memcloud._tcp.local."
    
    NodeB-->>mDNS: Announce presence
    mDNS-->>NodeA: ServiceResolved (NodeB info)
    
    NodeA->>NodeB: TCP Connect (port 8080)
    NodeB-->>NodeA: Connection Accepted
    
    NodeA->>NodeB: Hello { node_id, name }
    NodeB-->>NodeA: Welcome { peer_list }
    
    Note over NodeA,NodeB: Peers are now connected and can exchange blocks
```

---

## Module Structure

```
memcloud/
├── memnode/                 # Core daemon
│   ├── src/
│   │   ├── main.rs          # Entry point, CLI args
│   │   ├── blocks/          # Block storage & management
│   │   ├── discovery/       # mDNS peer discovery
│   │   ├── net/             # TCP transport layer
│   │   ├── peers/           # Peer connection management
│   │   ├── rpc/             # Local RPC server (Unix socket + JSON TCP)
│   │   └── metadata/        # Block metadata & indexing
│   └── Cargo.toml
│
├── memsdk/                  # Rust SDK
│   ├── src/lib.rs           # Client API
│   └── Cargo.toml
│
├── memcli/                  # Command-line interface
│   ├── src/main.rs          # CLI commands (store, load, peers, node)
│   └── Cargo.toml
│
├── js-sdk/                  # TypeScript SDK (npm: memcloud)
│   ├── src/
│   │   ├── api.ts           # MemCloud class
│   │   └── socket.ts        # TCP/Unix socket transport
│   └── package.json
│
└── installers/              # Service files
    ├── macos/               # launchd plist
    └── linux/               # systemd service
```

---

## Wire Protocol

### Local RPC (JSON over TCP/Unix Socket)

All local communication uses length-prefixed JSON:

```
┌────────────────┬────────────────────────────┐
│ Length (4 bytes, big-endian) │ JSON Body  │
└────────────────┴────────────────────────────┘
```

**Example Commands:**
```json
// Store
{ "Store": { "data": [72, 101, 108, 108, 111] } }

// Load
{ "Load": { "id": 12345678 } }

// Set Key-Value
{ "Set": { "key": "my-key", "data": [1, 2, 3] } }

// Get Key-Value
{ "Get": { "key": "my-key" } }

// List Peers
"ListPeers"
```

### Peer-to-Peer Protocol (Binary)

Inter-node communication uses a binary protocol with `bincode` serialization for efficiency:

```rust
enum Message {
    Hello { node_id: Uuid, name: String },
    StoreBlock { id: u64, data: Vec<u8> },
    RequestBlock { id: u64 },
    BlockData { id: u64, data: Vec<u8> },
    SetKey { key: String, data: Vec<u8> },
    GetKey { key: String },
    KeyFound { key: String, data: Option<Vec<u8>> },
    Ping,
    Pong,
}
```

---

## 🔒 Security & Authentication

MemCloud implements a **Noise-based (XX) Secure Protocol** over TCP.

### Key Features
*   **Transcript Hashing**: All handshake messages are hashed into a running state (`Transcript`).
*   **Signature Binding**: Signatures sign the *Transcript Hash*, not just a nonce, preventing parameter tampering (MITM).
*   **Encrypted Identities**: Identity public keys and names are exchanged only *after* an encrypted tunnel is established via ephemeral keys.
*   **Forward Secrecy**: Ephemeral X25519 keys are generated for every session.

### Authentication Flow (Noise XX)

```mermaid
sequenceDiagram
    participant A as Node A (Initiator)
    participant B as Node B (Responder)

    Note over A,B: TCP Connection

    rect rgb(255, 255, 240)
    Note right of A: 1. Hello (Cleartext)
    A->>B: [HELLO] Nonce_A, Ephemeral_Pub_A, Quota
    Note over A,B: Mix Hash(HelloA)
    end

    rect rgb(255, 255, 240)
    Note right of B: 2. Hello (Cleartext)
    B->>A: [HELLO] Nonce_B, Ephemeral_Pub_B, Quota
    Note over A,B: Mix Hash(HelloB)
    Note over A,B: Derive Handshake Keys (ECDH)
    end

    rect rgb(230, 255, 230)
    Note right of A: 3. Auth A (Encrypted)
    A->>B: Encrypt([AUTH] Identity_A, Signature_A(Transcript))
    Note over A,B: Mix Hash(AuthA)
    end

    rect rgb(230, 255, 230)
    Note right of B: 4. Auth B (Encrypted)
    B->>A: Encrypt([AUTH] Identity_B, Signature_B(Transcript))
    Note over A,B: Mix Hash(AuthB)
    end

    Note over A,B: Derive Traffic Keys (Split Direction)
    Note over A,B: 🔒 Secure Session Establised
```
