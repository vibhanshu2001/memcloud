use blake3::Hasher;

/// Manages the running hash of the handshake transcript.
/// This ensures that all messages exchanged are cryptographically bound
/// to the final session keys, preventing tampering and downgrade attacks.
pub struct Transcript {
    hasher: Hasher,
}

impl Transcript {
    /// Initialize a new transcript with a protocol discriminator.
    pub fn new(protocol_name: &str) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(protocol_name.as_bytes());
        Self { hasher }
    }

    /// Mix arbitrary data into the transcript.
    pub fn mix(&mut self, _label: &str, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Mix a public key into the transcript.
    pub fn mix_key(&mut self, key: &[u8; 32]) {
        self.hasher.update(key);
    }

    /// Get the current hash state as a 32-byte array.
    /// Used for signing challenges or deriving keys.
    /// Does NOT reset the hasher (it's a running hash).
    pub fn current_hash(&self) -> [u8; 32] {
        *self.hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_mix_changes_hash() {
        let mut t = Transcript::new("TestProto");
        let h1 = t.current_hash();
        
        t.mix("label", b"data");
        let h2 = t.current_hash();
        
        assert_ne!(h1, h2);
    }
    
    #[test]
    fn test_transcript_consistency() {
        let mut t1 = Transcript::new("TestProto");
        t1.mix("label", b"data");
        
        let mut t2 = Transcript::new("TestProto");
        t2.mix("label", b"data");
        
        assert_eq!(t1.current_hash(), t2.current_hash());
    }
}
