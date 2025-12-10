use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use chacha20poly1305::aead::{Aead, KeyInit}; 
use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use std::fmt;

pub struct SecureReader {
    inner: OwnedReadHalf,
    cipher: ChaCha20Poly1305,
    nonce_counter: u64,
}

impl fmt::Debug for SecureReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureReader")
            .field("nonce_counter", &self.nonce_counter)
            .finish()
    }
}

impl SecureReader {
    pub fn new(inner: OwnedReadHalf, key: &[u8; 32]) -> Self {
        Self {
            inner,
            cipher: ChaCha20Poly1305::new(Key::from_slice(key)),
            nonce_counter: 0,
        }
    }

    /// Reads a length-prefixed, encrypted frame and returns the decrypted plaintext.
    pub async fn recv_frame(&mut self) -> Result<Vec<u8>> {
        // 1. Read Length (4 bytes)
        let mut len_buf = [0u8; 4];
        self.inner.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // 2. Read Ciphertext (len bytes)
        let mut buf = vec![0u8; len];
        self.inner.read_exact(&mut buf).await?;

        // 3. Construct Nonce
        let mut nonce_bytes = [0u8; 12];
        // Use big-endian counter at the end
        nonce_bytes[4..12].copy_from_slice(&self.nonce_counter.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 4. Decrypt
        let plaintext = self.cipher.decrypt(nonce, buf.as_ref())
            .map_err(|_| anyhow::anyhow!("Decryption failed"))?;

        // Increment nonce
        self.nonce_counter += 1;

        Ok(plaintext)
    }
}

pub struct SecureWriter {
    inner: BufWriter<OwnedWriteHalf>,
    cipher: ChaCha20Poly1305,
    nonce_counter: u64,
}

impl fmt::Debug for SecureWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureWriter")
            .field("nonce_counter", &self.nonce_counter)
            .finish()
    }
}

impl SecureWriter {
    pub fn new(inner: BufWriter<OwnedWriteHalf>, key: &[u8; 32]) -> Self {
        Self {
            inner,
            cipher: ChaCha20Poly1305::new(Key::from_slice(key)),
            nonce_counter: 0,
        }
    }
    
    // Helper to accept raw inner without bufwriter wrapping (it wraps it internally)
    pub fn from_raw(inner: OwnedWriteHalf, key: &[u8; 32]) -> Self {
         Self::new(BufWriter::new(inner), key)
    }

    /// Encrypts data and sends it as a length-prefixed frame.
    pub async fn send_frame(&mut self, data: &[u8]) -> Result<()> {
        // 1. Construct Nonce
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&self.nonce_counter.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 2. Encrypt
        let ciphertext = self.cipher.encrypt(nonce, data)
            .map_err(|_| anyhow::anyhow!("Encryption failed"))?;

        // 3. Send Length
        let len = ciphertext.len() as u32;
        self.inner.write_all(&len.to_be_bytes()).await?;

        // 4. Send Ciphertext
        self.inner.write_all(&ciphertext).await?;
        self.inner.flush().await?;

        // Increment nonce
        self.nonce_counter += 1;

        Ok(())
    }
}
