use serde::{Serialize, Deserialize};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use x25519_dalek::{EphemeralSecret, PublicKey as XPublicKey, StaticSecret};
use rand::rngs::OsRng;
use anyhow::{Result, bail, Context};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use crate::peers::PeerMetadata;

#[derive(Serialize, Deserialize, Debug)]
pub enum HandshakeMessage {
    Hello(AuthHello),
    Challenge(AuthChallenge),
    Response(AuthResponse),
    Finish(AuthFinish),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthHello {
    pub version: u16,
    pub node_id: Uuid,
    pub pub_key: [u8; 32], // Ed25519
    pub nonce: [u8; 32],
    pub name: String,
    pub ram_quota: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthChallenge {
    pub pub_key: [u8; 32], // Ed25519 (Responder's)
    pub nonce: [u8; 32],
    pub challenge: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthResponse {
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
    pub eph_pub: [u8; 32], // X25519
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthFinish {
    pub eph_pub: [u8; 32], // X25519
}

#[derive(Clone)]
pub struct Identity {
    pub keypair: SigningKey,
    pub node_id: Uuid,
    pub name: String,
}

impl Identity {
    pub fn new(node_id: Uuid, name: String) -> Self {
        let mut csprng = OsRng;
        let keypair = SigningKey::generate(&mut csprng);
        Self {
            keypair,
            node_id,
            name,
        }
    }
    
    pub fn public_key(&self) -> VerifyingKey {
        self.keypair.verifying_key()
    }
}

pub struct Session {
    pub send_key: [u8; 32],
    pub recv_key: [u8; 32],
    pub peer_id: Uuid,
    pub peer_name: String,
    pub peer_quota: u64,
}

// Handshake Logic

pub async fn handshake_initiator(
    stream: &mut TcpStream,
    identity: &Identity,
    ram_quota: u64,
) -> Result<Session> {
    let mut csprng = OsRng;
    
    // 1. Send Hello
    let nonce_a: [u8; 32] = rand::Rng::gen(&mut csprng);
    let hello = HandshakeMessage::Hello(AuthHello {
        version: 1,
        node_id: identity.node_id,
        pub_key: identity.public_key().to_bytes(),
        nonce: nonce_a,
        name: identity.name.clone(),
        ram_quota,
    });
    send_msg(stream, &hello).await?;

    // 2. Expect Challenge
    let challenge_msg = recv_msg(stream).await?;
    let (nonce_b, challenge, _pub_key_b) = match challenge_msg {
        HandshakeMessage::Challenge(c) => (c.nonce, c.challenge, c.pub_key),
        _ => bail!("Expected AuthChallenge, got {:?}", challenge_msg),
    };

    // Verify Responder (B)
    
    // 3. Send Response
    // Sign challenge
    let signature = identity.keypair.sign(&challenge);
    
    // Gen Ephemeral
    let eph_secret = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = XPublicKey::from(&eph_secret);
    
    let resp = HandshakeMessage::Response(AuthResponse {
        signature: signature.to_bytes(),
        eph_pub: *eph_pub.as_bytes(),
    });
    send_msg(stream, &resp).await?;
    
    // 4. Expect Finish
    let finish_msg = recv_msg(stream).await?;
    let eph_pub_b_bytes = match finish_msg {
        HandshakeMessage::Finish(f) => f.eph_pub,
        _ => bail!("Expected AuthFinish, got {:?}", finish_msg),
    };
    
    let eph_pub_b = XPublicKey::from(eph_pub_b_bytes);
    
    // Derive Shared Secret
    let shared_secret = eph_secret.diffie_hellman(&eph_pub_b);
    
    // Derive Session Keys (KDF)
    let mut hasher = blake3::Hasher::new();
    hasher.update(shared_secret.as_bytes());
    hasher.update(&nonce_a);
    hasher.update(&nonce_b);
    let session_key = hasher.finalize(); 
    
    let mut hasher_send = blake3::Hasher::new();
    hasher_send.update(session_key.as_bytes());
    hasher_send.update(b"initiator_to_responder");
    let send_key = hasher_send.finalize();

    let mut hasher_recv = blake3::Hasher::new();
    hasher_recv.update(session_key.as_bytes());
    hasher_recv.update(b"responder_to_initiator");
    let recv_key = hasher_recv.finalize();

    let peer_id = Uuid::nil(); 
    
    Ok(Session {
        send_key: *send_key.as_bytes(),
        recv_key: *recv_key.as_bytes(),
        peer_id,
        peer_name: "Unknown".to_string(), 
        peer_quota: 0, 
    })
}

pub async fn handshake_responder(
    stream: &mut TcpStream,
    identity: &Identity,
) -> Result<Session> {
    let mut csprng = OsRng;

    // 1. Expect Hello
    let hello_msg = recv_msg(stream).await?;
    let (nonce_a, pub_key_a_bytes, peer_id, peer_name, peer_quota) = match hello_msg {
        HandshakeMessage::Hello(h) => (h.nonce, h.pub_key, h.node_id, h.name, h.ram_quota),
        _ => bail!("Expected AuthHello, got {:?}", hello_msg),
    };
    
    // TODO: Verify A's public key against trusted list
    let pub_key_a = VerifyingKey::from_bytes(&pub_key_a_bytes)?;

    // 2. Send Challenge
    let nonce_b: [u8; 32] = rand::Rng::gen(&mut csprng);
    let challenge: [u8; 32] = rand::Rng::gen(&mut csprng);
    
    let challenge_pkt = HandshakeMessage::Challenge(AuthChallenge {
        pub_key: identity.public_key().to_bytes(),
        nonce: nonce_b,
        challenge,
    });
    send_msg(stream, &challenge_pkt).await?;
    
    // 3. Expect Response
    let resp_msg = recv_msg(stream).await?;
    let (signature_bytes, eph_pub_a_bytes) = match resp_msg {
        HandshakeMessage::Response(r) => (r.signature, r.eph_pub),
        _ => bail!("Expected AuthResponse, got {:?}", resp_msg),
    };
    
    let signature = Signature::from_bytes(&signature_bytes);
    
    // Verify Signature
    pub_key_a.verify(&challenge, &signature).context("Invalid signature from peer")?;
    
    // 4. Send Finish
    let eph_secret = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = XPublicKey::from(&eph_secret);
    
    let finish = HandshakeMessage::Finish(AuthFinish {
        eph_pub: *eph_pub.as_bytes(),
    });
    send_msg(stream, &finish).await?;
    
    // Derive Keys
    let eph_pub_a = XPublicKey::from(eph_pub_a_bytes);
    let shared_secret = eph_secret.diffie_hellman(&eph_pub_a);
    
    let mut hasher = blake3::Hasher::new();
    hasher.update(shared_secret.as_bytes());
    hasher.update(&nonce_a);
    hasher.update(&nonce_b);
    let session_key = hasher.finalize();

    // Derive send/recv keys
    let mut hasher_recv = blake3::Hasher::new();
    hasher_recv.update(session_key.as_bytes());
    hasher_recv.update(b"initiator_to_responder");
    let recv_key = hasher_recv.finalize();

    let mut hasher_send = blake3::Hasher::new();
    hasher_send.update(session_key.as_bytes());
    hasher_send.update(b"responder_to_initiator");
    let send_key = hasher_send.finalize();

    Ok(Session {
        send_key: *send_key.as_bytes(),
        recv_key: *recv_key.as_bytes(),
        peer_id,
        peer_name,
        peer_quota,
    })
}

async fn send_msg(stream: &mut TcpStream, msg: &HandshakeMessage) -> Result<()> {
    let bytes = bincode::serialize(msg)?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

async fn recv_msg(stream: &mut TcpStream) -> Result<HandshakeMessage> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let msg: HandshakeMessage = bincode::deserialize(&buf)?;
    Ok(msg)
}
