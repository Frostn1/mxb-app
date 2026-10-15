use std::collections::HashMap;

use aes::Aes128;
use cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use json::Value;
use serde::{Deserialize, Serialize};

use crate::fingerprint::NodeFingerprint;
use crate::Result;

/// Represents the node's attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeAttributes {
    /// The name of the node.
    #[serde(rename = "n")]
    pub name: String,
    /// The encoded fingerprint for the node.
    #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// The last modified date of the node.
    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    /// Catch-all for the remaining fields (if any).
    #[serde(flatten)]
    pub other: HashMap<String, Value>,
}

impl NodeAttributes {
    pub(crate) fn decrypt_and_unpack(file_key: &[u8; 16], buffer: &mut [u8]) -> Result<Self> {
        let mut cbc = cbc::Decryptor::<Aes128>::new(file_key.into(), &<_>::default());
        for chunk in buffer.chunks_exact_mut(16) {
            cbc.decrypt_block_mut(chunk.into());
        }

        // Vendored change: upstream asserts here, and a node whose key does not fit this link
        // aborted the whole app. A bad node is an error the caller can skip.
        if buffer.len() < 4 || &buffer[..4] != b"MEGA" {
            return Err(crate::error::Error::InvalidNodeAttributes);
        }

        let len = buffer.iter().take_while(|it| **it != b'\0').count();
        let attrs = json::from_slice(&buffer[4..len])?;

        Ok(attrs)
    }

    pub(crate) fn pack_and_encrypt(&self, file_key: &[u8; 16]) -> Result<Vec<u8>> {
        let mut buffer = b"MEGA".to_vec();
        json::to_writer(&mut buffer, self)?;

        let padding_len = (16 - buffer.len() % 16) % 16;
        buffer.extend(std::iter::repeat(b'\0').take(padding_len));

        let mut cbc = cbc::Encryptor::<Aes128>::new(file_key.into(), &<_>::default());
        for chunk in buffer.chunks_exact_mut(16) {
            cbc.encrypt_block_mut(chunk.into());
        }

        Ok(buffer)
    }

    pub(crate) fn extract_fingerprint(&self) -> Option<NodeFingerprint> {
        let checksum = self.fingerprint.as_deref()?;
        NodeFingerprint::deserialize(checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_key_is_an_error_not_a_panic() {
        let attrs = NodeAttributes { name: "a.zip".into(), fingerprint: None, modified_at: None, other: Default::default() };
        let good = [7u8; 16];
        let mut packed = attrs.pack_and_encrypt(&good).unwrap();
        assert!(NodeAttributes::decrypt_and_unpack(&good, &mut packed.clone()).is_ok());
        let bad = [9u8; 16];
        assert!(NodeAttributes::decrypt_and_unpack(&bad, &mut packed).is_err());
        assert!(NodeAttributes::decrypt_and_unpack(&good, &mut []).is_err());
    }
}
