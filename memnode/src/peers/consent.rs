use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use uuid::Uuid;
use std::time::Instant;
use anyhow::Result;
use log::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsentDecision {
    Pending,
    ApprovedOnce,
    ApprovedAndTrusted,
    Denied,
}

#[derive(Debug, Clone)]
pub struct PendingConsent {
    pub session_id: String,
    pub peer_pubkey: String,
    pub peer_name: String,
    pub quota: u64,
    pub created_at: u64,
}

pub struct ConsentManager {
    pending: Arc<Mutex<HashMap<String, PendingConsent>>>,
    notifier: broadcast::Sender<(String, ConsentDecision)>,
}

impl ConsentManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            notifier: tx,
        }
    }

    pub fn request_consent(&self, session_id: String, peer_pubkey: String, peer_name: String, quota: u64) {
        let mut lock = self.pending.lock().unwrap();
        lock.insert(session_id.clone(), PendingConsent {
            session_id,
            peer_pubkey: peer_pubkey.clone(),
            peer_name: peer_name.clone(),
            quota,
            created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        });
        info!("Pending consent created for peer {} (key={}, quota={} bytes)", peer_name, peer_pubkey, quota);  
    }

    pub async fn wait_for_decision(&self, session_id: &str) -> ConsentDecision {
        let mut rx = self.notifier.subscribe();
        loop {
            match rx.recv().await {
                Ok((id, decision)) => {
                    if id == session_id {
                        return decision;
                    }
                }
                Err(e) => {
                    warn!("Consent broadcast error: {}", e);
                    return ConsentDecision::Denied; // Fail safe
                }
            }
        }
    }

    pub fn resolve(&self, session_id: &str, decision: ConsentDecision) -> Result<()> {
        let mut lock = self.pending.lock().unwrap();
        if lock.remove(session_id).is_some() {
            // Notify waiters
            let _ = self.notifier.send((session_id.to_string(), decision));
            Ok(())
        } else {
            anyhow::bail!("No pending request for session {}", session_id);
        }
    }

    pub fn get_pending_list(&self) -> Vec<PendingConsent> {
        let lock = self.pending.lock().unwrap();
        lock.values().cloned().collect()
    }
}
