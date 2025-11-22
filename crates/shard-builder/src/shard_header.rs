use eyre::{Result, ensure};

/// Shard file header (32 bytes total)
/// Matches the spec from mds/architecture/prover_architecture.md:
/// - shard_id: u32 (4 bytes)
/// - start_position: u64 (8 bytes)
/// - end_position: u64 (8 bytes)
/// - leaf_count: u32 (4 bytes)
/// - reserved: [u8; 8] (8 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardHeader {
    pub shard_id: u32,
    pub start_position: u64,
    pub end_position: u64,
    pub leaf_count: u32,
    pub reserved: [u8; 8],
}

impl ShardHeader {
    pub const HEADER_SIZE: usize = 32;

    /// Create a new shard header
    pub fn new(shard_id: u32, start_position: u64, end_position: u64, leaf_count: u32) -> Self {
        Self {
            shard_id,
            start_position,
            end_position,
            leaf_count,
            reserved: [0u8; 8],
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> [u8; Self::HEADER_SIZE] {
        let mut bytes = [0u8; Self::HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.shard_id.to_le_bytes());
        bytes[4..12].copy_from_slice(&self.start_position.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.end_position.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.leaf_count.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.reserved);
        bytes
    }

    /// Deserialize header from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= Self::HEADER_SIZE,
            "Header bytes too short: expected {} bytes, got {}",
            Self::HEADER_SIZE,
            bytes.len()
        );

        let shard_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let start_position = u64::from_le_bytes([
            bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
        ]);
        let end_position = u64::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
        ]);
        let leaf_count = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&bytes[24..32]);

        Ok(Self {
            shard_id,
            start_position,
            end_position,
            leaf_count,
            reserved,
        })
    }

    /// Validate header consistency
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start_position < self.end_position,
            "start_position ({}) must be less than end_position ({})",
            self.start_position,
            self.end_position
        );

        let shard_size = self.end_position - self.start_position;
        ensure!(
            self.leaf_count as u64 <= shard_size,
            "leaf_count ({}) exceeds shard size ({})",
            self.leaf_count,
            shard_size
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = ShardHeader::new(754, 49453056, 49518624, 39405);
        let bytes = header.to_bytes();
        let restored = ShardHeader::from_bytes(&bytes).unwrap();
        assert_eq!(header, restored);
    }

    #[test]
    fn test_header_validation() {
        let valid = ShardHeader::new(0, 0, 65536, 1000);
        assert!(valid.validate().is_ok());

        let invalid_start = ShardHeader::new(0, 100, 50, 10);
        assert!(invalid_start.validate().is_err());

        let invalid_count = ShardHeader::new(0, 0, 65536, 70000);
        assert!(invalid_count.validate().is_err());
    }
}
