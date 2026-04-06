use snow::{Builder, TransportState, HandshakeState};
use thiserror::Error;
use crate::hive::HiveCommand;

#[derive(Error, Debug)]
pub enum TunnelError {
    #[error("Noise framework error: {0}")]
    NoiseError(#[from] snow::Error),
    #[error("Bincode error: {0}")]
    BincodeError(#[from] bincode::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Re-keying threshold met")]
    RekeyRequired,
}

pub struct SovereignTunnel {
    pub noise_state: Option<HandshakeState>,
    pub transport: Option<TransportState>,
    pub transfer_count: u64,
}

impl SovereignTunnel {
    pub fn init_initiator(local_secret: &[u8], remote_pubkey: &[u8]) -> Result<Self, TunnelError> {
        let builder = Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
        let mut key = [0u8; 32];
        key.copy_from_slice(&local_secret[..32]);

        let handshake = builder
            .local_private_key(&key)
            .remote_public_key(remote_pubkey)
            .build_initiator()?;

        Ok(Self { noise_state: Some(handshake), transport: None, transfer_count: 0 })
    }

    pub fn init_responder(local_secret: &[u8], _remote_pubkey_expected: &[u8]) -> Result<Self, TunnelError> {
        let builder = Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
        let mut key = [0u8; 32];
        key.copy_from_slice(&local_secret[..32]);
        
        let handshake = builder
            .local_private_key(&key)
            .build_responder()?;
            
        Ok(Self { noise_state: Some(handshake), transport: None, transfer_count: 0 })
    }

    pub fn to_transport(&mut self) -> Result<(), TunnelError> {
        if let Some(hs) = self.noise_state.take() {
            self.transport = Some(hs.into_transport_mode()?);
        }
        Ok(())
    }
    
    pub fn encrypt_command(&mut self, cmd: &HiveCommand) -> Result<Vec<u8>, TunnelError> {
        let payload = bincode::serialize(cmd)?;
        self.transfer_count += payload.len() as u64;
        if self.transfer_count > 1_000_000_000 {
            return Err(TunnelError::RekeyRequired);
        }
        
        let mut final_out = Vec::new();
        let chunk_size = 65000;
        
        for chunk in payload.chunks(chunk_size) {
            let mut out = vec![0u8; chunk.len() + 1024]; 
            let len = if let Some(transport) = &mut self.transport {
                transport.write_message(chunk, &mut out)?
            } else if let Some(noise) = &mut self.noise_state {
                noise.write_message(chunk, &mut out)?
            } else {
                return Err(TunnelError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Tunnel not initialized")));
            };
            
            final_out.extend_from_slice(&(len as u32).to_be_bytes());
            final_out.extend_from_slice(&out[..len]);
        }
        
        Ok(final_out)
    }

    pub fn decrypt_payload(&mut self, payload: &[u8]) -> Result<HiveCommand, TunnelError> {
        let mut plaintext = Vec::new();
        let mut offset = 0;
        
        while offset < payload.len() {
            if offset + 4 > payload.len() {
                return Err(TunnelError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, "Truncated frame length")));
            }
            let mut len_buf = [0u8; 4];
            len_buf.copy_from_slice(&payload[offset..offset+4]);
            let frame_len = u32::from_be_bytes(len_buf) as usize;
            offset += 4;
            
            if offset + frame_len > payload.len() {
                return Err(TunnelError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, "Truncated frame content")));
            }
            let frame = &payload[offset..offset+frame_len];
            offset += frame_len;
            
            let mut out = vec![0u8; frame.len()];
            let len = if let Some(transport) = &mut self.transport {
                transport.read_message(frame, &mut out)?
            } else if let Some(noise) = &mut self.noise_state {
                noise.read_message(frame, &mut out)?
            } else {
                return Err(TunnelError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Tunnel not initialized")));
            };
            plaintext.extend_from_slice(&out[..len]);
        }
        
        let cmd: HiveCommand = bincode::deserialize(&plaintext)?;
        Ok(cmd)
    }
}
