use serde::{Serialize, Deserialize};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use x25519_dalek::{EphemeralSecret, PublicKey as XPublicKey, StaticSecret};
use rand::rngs::OsRng;
use anyhow::{Result, bail, Context};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use crate::peers::PeerMetadata;
use super::transcript::Transcript;
use crate::peers::trusted::TrustedStore;
use crate::peers::consent::{ConsentManager, ConsentDecision};
use std::sync::Arc;
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit};
use log::{info, error};

// --- Wire Messages ---

#[derive(Serialize, Deserialize, Debug)]
pub enum HandshakeMessage {
    Hello(HandshakeHello),
    Auth(Vec<u8>), // Encrypted HandshakeAuth
    ConsentRequired { reason: String },
    ConsentDenied,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeHello {
    pub version: u16,
    pub nonce: [u8; 32],
    pub eph_pub: [u8; 32], // X25519
    pub quota: u64,
    pub total_memory: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeAuth {
    pub node_id: Uuid,
    pub pub_key: [u8; 32], // Ed25519 Static Identity
    pub name: String,
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>, // Signature of Transcript
}

// --- Internal Structs ---

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
    pub peer_total_memory: u64,
}

// --- Handshake Implementation ---

pub async fn handshake_initiator(
    stream: &mut TcpStream,
    identity: &Identity,
    ram_quota: u64,
    total_memory: u64,
    mut on_consent_required: impl FnMut(),
) -> Result<Session> {
    let mut transcript = Transcript::new("MemCloud-v2");

    let eph_secret = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = XPublicKey::from(&eph_secret);
    let nonce_a: [u8; 32] = rand::random();

    let hello_a = HandshakeHello {
        version: 2,
        nonce: nonce_a,
        eph_pub: *eph_pub.as_bytes(),
        quota: ram_quota,
        total_memory,
    };
    send_msg(stream, &HandshakeMessage::Hello(hello_a)).await?;
    
    let hello_bytes = bincode::serialize(&HandshakeMessage::Hello(HandshakeHello {
        version: 2, nonce: nonce_a, eph_pub: *eph_pub.as_bytes(), quota: ram_quota, total_memory
    }))?;
    transcript.mix("hello_a", &hello_bytes);

    let msg = recv_msg(stream).await?;
    let (hello_b_bytes, hello_b) = match msg {
        (b, HandshakeMessage::Hello(h)) => (b, h),
        (_, m) => bail!("Expected Hello, got {:?}", m),
    };
    transcript.mix("hello_b", &hello_b_bytes);

    let eph_pub_b = XPublicKey::from(hello_b.eph_pub);
    
    let shared_secret = eph_secret.diffie_hellman(&eph_pub_b);
    let handshake_key = derive_key("handshake_key", &shared_secret.to_bytes(), &transcript.current_hash());
    
    let sig_payload = transcript.current_hash();
    let signature = identity.keypair.sign(&sig_payload);
    
    let auth_a = HandshakeAuth {
        node_id: identity.node_id,
        pub_key: identity.public_key().to_bytes(),
        name: identity.name.clone(),
        signature: signature.to_bytes().to_vec(),
    };
    
    let auth_a_bytes = bincode::serialize(&auth_a)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&handshake_key));
    let nonce = Nonce::from_slice(&[0u8; 12]);
    let ciphertext_a = cipher.encrypt(nonce, auth_a_bytes.as_ref())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
        
    let auth_msg_out = HandshakeMessage::Auth(ciphertext_a.clone());
    send_msg(stream, &auth_msg_out).await?;
    
    let auth_a_wire_bytes = bincode::serialize(&auth_msg_out)?;
    transcript.mix("auth_a", &auth_a_wire_bytes);
    
    // Check if peer requires consent
    let mut msg = recv_msg(stream).await?;
    
    // Handle Consent Loop
    loop {
        match msg {
            (b, HandshakeMessage::ConsentRequired { reason }) => {
                info!("Peer requires consent: {}", reason);
                on_consent_required();
                msg = recv_msg(stream).await?;
            }
            (b, HandshakeMessage::ConsentDenied) => {
                bail!("Connection rejected by peer user.");
            }
            (b, HandshakeMessage::Auth(c)) => {
                // This is effectively "Granted"
                msg = (b, HandshakeMessage::Auth(c));
                break;
            }
            (_, m) => bail!("Unexpected message during auth wait: {:?}", m),
        }
    }

    let (auth_b_msg_bytes, ciphertext_b) = match msg {
        (b, HandshakeMessage::Auth(c)) => (b, c),
        _ => unreachable!(),
    };
    
    let nonce_b_dec = Nonce::from_slice(&[0,0,0,0,0,0,0,0,0,0,0,1]); 
    let auth_b_data = cipher.decrypt(nonce_b_dec, ciphertext_b.as_ref())
         .map_err(|_| anyhow::anyhow!("Decryption of peer auth failed"))?;
         
    let auth_b: HandshakeAuth = bincode::deserialize(&auth_b_data)?;
    
    let peer_key = VerifyingKey::from_bytes(&auth_b.pub_key)?;
    
    if auth_b.signature.len() != 64 {
        bail!("Invalid signature length");
    }
    let peer_signature = Signature::from_bytes(auth_b.signature.as_slice().try_into().unwrap());
    peer_key.verify(&transcript.current_hash(), &peer_signature)
        .context("Peer signature verification failed")?;

    transcript.mix("auth_b", &auth_b_msg_bytes);

    let final_hash = transcript.current_hash();
    let send_key = derive_key("traffic_a", &shared_secret.to_bytes(), &final_hash);
    let recv_key = derive_key("traffic_b", &shared_secret.to_bytes(), &final_hash);

    Ok(Session {
        send_key, // Initiator (A) sends with Key A
        recv_key, // Initiator (A) recvs with Key B
        peer_id: auth_b.node_id,
        peer_name: auth_b.name,
        peer_quota: hello_b.quota,
        peer_total_memory: hello_b.total_memory,
    })
}

pub async fn handshake_responder(
    stream: &mut TcpStream,
    identity: &Identity,
    trusted_store: Arc<TrustedStore>,
    consent_manager: Arc<ConsentManager>,
    ram_quota: u64,
    total_memory: u64,
) -> Result<Session> {
    let mut transcript = Transcript::new("MemCloud-v2");

    let msg = recv_msg(stream).await?;
    let (hello_a_bytes, hello_a) = match msg {
        (b, HandshakeMessage::Hello(h)) => (b, h),
        (_, m) => bail!("Expected Hello, got {:?}", m),
    };
    transcript.mix("hello_a", &hello_a_bytes);

    let eph_pub_a = XPublicKey::from(hello_a.eph_pub);

    let eph_secret = EphemeralSecret::random_from_rng(OsRng);
    let eph_pub = XPublicKey::from(&eph_secret);
    let nonce_b: [u8; 32] = rand::random();
    
    let hello_b = HandshakeHello {
        version: 2,
        nonce: nonce_b,
        eph_pub: *eph_pub.as_bytes(),
        quota: ram_quota,
        total_memory,
    };
    send_msg(stream, &HandshakeMessage::Hello(hello_b)).await?;
    
    let hello_b_bytes = bincode::serialize(&HandshakeMessage::Hello(HandshakeHello {
        version: 2, nonce: nonce_b, eph_pub: *eph_pub.as_bytes(), quota: ram_quota, total_memory
    }))?;
    transcript.mix("hello_b", &hello_b_bytes);

    let shared_secret = eph_secret.diffie_hellman(&eph_pub_a);
    let handshake_key = derive_key("handshake_key", &shared_secret.to_bytes(), &transcript.current_hash());

    let msg = recv_msg(stream).await?;
    let (auth_a_msg_bytes, ciphertext_a) = match msg {
        (b, HandshakeMessage::Auth(c)) => (b, c),
        (_, m) => bail!("Expected Auth, got {:?}", m),
    };
    
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&handshake_key));
    let nonce_a_dec = Nonce::from_slice(&[0u8; 12]); 
    let auth_a_data = cipher.decrypt(nonce_a_dec, ciphertext_a.as_ref())
         .map_err(|_| anyhow::anyhow!("Decryption of peer auth failed"))?;
    let auth_a: HandshakeAuth = bincode::deserialize(&auth_a_data)?;
    
    let peer_key = VerifyingKey::from_bytes(&auth_a.pub_key)?;
    if auth_a.signature.len() != 64 {
        bail!("Invalid signature length");
    }
    let peer_signature = Signature::from_bytes(auth_a.signature.as_slice().try_into().unwrap());
    peer_key.verify(&transcript.current_hash(), &peer_signature)
        .context("Peer signature verification failed")?;

    let peer_pub_key_hex = hex::encode(auth_a.pub_key);
    if !trusted_store.is_trusted(&peer_pub_key_hex) {
        info!("Peer {} ({}) is unknown. Requesting consent...", auth_a.name, peer_pub_key_hex);
        
        send_msg(stream, &HandshakeMessage::ConsentRequired { reason: "untrusted_peer".to_string() }).await?;

        let session_id = Uuid::new_v4().to_string();
        consent_manager.request_consent(session_id.clone(), peer_pub_key_hex.clone(), auth_a.name.clone(), hello_a.quota);
        
        // Wait
        let decision = consent_manager.wait_for_decision(&session_id).await;
        
        match decision {
            ConsentDecision::ApprovedOnce => {
                info!("Consent granted (once) for {}", auth_a.name);
            }
            ConsentDecision::ApprovedAndTrusted => {
                info!("Consent granted (trusted) for {}", auth_a.name);
                trusted_store.add_trusted(peer_pub_key_hex, auth_a.name.clone())?;
            }
            ConsentDecision::Denied | ConsentDecision::Pending => {
                info!("Consent denied for {}", auth_a.name);
                send_msg(stream, &HandshakeMessage::ConsentDenied).await?;
                bail!("Connection denied by user");
            }
        }
    } else {
        info!("Peer {} is trusted. Proceeding.", auth_a.name);
    }
        
    transcript.mix("auth_a", &auth_a_msg_bytes);
    
    let sig_payload = transcript.current_hash();
    let signature = identity.keypair.sign(&sig_payload);
    
    let auth_b = HandshakeAuth {
        node_id: identity.node_id,
        pub_key: identity.public_key().to_bytes(),
        name: identity.name.clone(),
        signature: signature.to_bytes().to_vec(),
    };
    
    let auth_b_bytes = bincode::serialize(&auth_b)?;
    let nonce_b_enc = Nonce::from_slice(&[0,0,0,0,0,0,0,0,0,0,0,1]); // Nonce 1
    let ciphertext_b = cipher.encrypt(nonce_b_enc, auth_b_bytes.as_ref())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
        
    send_msg(stream, &HandshakeMessage::Auth(ciphertext_b.clone())).await?;
    
    let sent_auth_b_bytes = bincode::serialize(&HandshakeMessage::Auth(ciphertext_b))?;
    transcript.mix("auth_b", &sent_auth_b_bytes);
    
    let final_hash = transcript.current_hash();
    let send_key = derive_key("traffic_b", &shared_secret.to_bytes(), &final_hash); // B sends on Key B
    let recv_key = derive_key("traffic_a", &shared_secret.to_bytes(), &final_hash); // B recvs on Key A
    
    Ok(Session {
        send_key,
        recv_key,
        peer_id: auth_a.node_id,
        peer_name: auth_a.name,
        peer_quota: hello_a.quota,
        peer_total_memory: hello_a.total_memory,
    })
}


// --- Helpers ---

fn derive_key(label: &str, shared: &[u8], context: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(shared);
    hasher.update(context);
    hasher.update(label.as_bytes());
    *hasher.finalize().as_bytes()
}

// Helper that returns raw bytes + deserialized msg for mixing
async fn recv_msg(stream: &mut TcpStream) -> Result<(Vec<u8>, HandshakeMessage)> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let msg: HandshakeMessage = bincode::deserialize(&buf)?;
    Ok((buf, msg))
}

async fn send_msg(stream: &mut TcpStream, msg: &HandshakeMessage) -> Result<()> {
    let bytes = bincode::serialize(msg)?;
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}
