use eyre::{Result, eyre};
use jubjub::Fr as JubjubFr;
use orchard::{
    Address as OrchardAddress,
    note::{self, RandomSeed as OrchardRandomSeed, Rho},
    value::NoteValue as OrchardNoteValue,
};
use sapling::{
    Note as SaplingNote, PaymentAddress as SaplingPaymentAddress, Rseed as SaplingRseed,
    value::NoteValue as SaplingNoteValue,
};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use zcash_client_backend::wallet::ReceivedNote;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::MAIN_NETWORK;
use zip32::Scope;

#[derive(Clone, Serialize, Deserialize)]
pub struct MerkleWitness {
    pub position: u64,
    pub path: Vec<[u8; 32]>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ScopeTag {
    External,
    Internal,
}

impl From<Scope> for ScopeTag {
    fn from(scope: Scope) -> Self {
        match scope {
            Scope::External => ScopeTag::External,
            Scope::Internal => ScopeTag::Internal,
        }
    }
}

impl From<ScopeTag> for Scope {
    fn from(value: ScopeTag) -> Self {
        match value {
            ScopeTag::External => Scope::External,
            ScopeTag::Internal => Scope::Internal,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum SaplingRseedRepr {
    BeforeZip212([u8; 32]),
    AfterZip212([u8; 32]),
}

impl From<&SaplingRseed> for SaplingRseedRepr {
    fn from(rseed: &SaplingRseed) -> Self {
        match rseed {
            SaplingRseed::BeforeZip212(fr) => SaplingRseedRepr::BeforeZip212(fr.to_bytes()),
            SaplingRseed::AfterZip212(bytes) => SaplingRseedRepr::AfterZip212(*bytes),
        }
    }
}

impl SaplingRseedRepr {
    pub fn into_rseed(self) -> Result<SaplingRseed> {
        match self {
            SaplingRseedRepr::BeforeZip212(bytes) => {
                let fr = Option::<JubjubFr>::from(JubjubFr::from_bytes(&bytes))
                    .ok_or_else(|| eyre!("invalid Sapling before-ZIP212 rseed"))?;
                Ok(SaplingRseed::BeforeZip212(fr))
            }
            SaplingRseedRepr::AfterZip212(bytes) => Ok(SaplingRseed::AfterZip212(bytes)),
        }
    }
}

/// Each shielded commitment being proven is either unspent at the time of the snapshot, or from
/// after the snapshot, used to prove ownership of transparent UTXOs.
#[derive(Clone, Serialize, Deserialize)]
pub enum ShieldedHoldingsData {
    /// Proof that the shielded commitment is included and unspent at the time of the snapshot.
    SnapshotHoldings(MerkleWitness, ExclusionProof),
    /// Proof that a corresponding unspent transparent UTXO was present at the time of the snapshot.
    TransparentShielding(TransparentShieldingTxRecord),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SaplingNoteRecord {
    pub scope: ScopeTag,
    pub value: u64,
    pub recipient: Vec<u8>,
    pub rseed: SaplingRseedRepr,
    pub extra_data: ShieldedHoldingsData,
}

impl SaplingNoteRecord {
    pub fn from_received<N>(
        note: &ReceivedNote<N, SaplingNote>,
        extra_data: ShieldedHoldingsData,
    ) -> Self {
        SaplingNoteRecord {
            scope: note.spending_key_scope().into(),
            value: note.note().value().inner(),
            recipient: note.note().recipient().to_bytes().to_vec(),
            rseed: SaplingRseedRepr::from(note.note().rseed()),
            extra_data,
        }
    }

    pub fn rebuild_note(&self) -> Result<SaplingNote> {
        let recipient_bytes: [u8; 43] = self
            .recipient
            .as_slice()
            .try_into()
            .map_err(|_| eyre!("invalid Sapling recipient length"))?;
        let recipient = SaplingPaymentAddress::from_bytes(&recipient_bytes)
            .ok_or_else(|| eyre!("invalid Sapling recipient bytes"))?;
        let value = SaplingNoteValue::from_raw(self.value);
        let rseed = self.rseed.clone().into_rseed()?;
        Ok(SaplingNote::from_parts(recipient, value, rseed))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OrchardNoteRecord {
    pub scope: ScopeTag,
    pub value: u64,
    pub rho: [u8; 32],
    pub rseed: [u8; 32],
    pub recipient: Vec<u8>,
    pub extra_data: ShieldedHoldingsData,
}

impl OrchardNoteRecord {
    pub fn from_received<N>(
        note: &ReceivedNote<N, note::Note>,
        extra_data: ShieldedHoldingsData,
    ) -> Self {
        OrchardNoteRecord {
            scope: note.spending_key_scope().into(),
            value: note.note().value().inner(),
            rho: note.note().rho().to_bytes(),
            rseed: *note.note().rseed().as_bytes(),
            recipient: note.note().recipient().to_raw_address_bytes().to_vec(),
            extra_data,
        }
    }

    pub fn rebuild_note(&self) -> Result<note::Note> {
        let recipient_bytes: [u8; 43] = self
            .recipient
            .as_slice()
            .try_into()
            .map_err(|_| eyre!("invalid Orchard recipient length"))?;
        let address = Option::<OrchardAddress>::from(OrchardAddress::from_raw_address_bytes(
            &recipient_bytes,
        ))
        .ok_or_else(|| eyre!("invalid Orchard recipient bytes"))?;
        let rho = Option::<Rho>::from(Rho::from_bytes(&self.rho))
            .ok_or_else(|| eyre!("invalid Orchard rho"))?;
        let rseed =
            Option::<OrchardRandomSeed>::from(OrchardRandomSeed::from_bytes(self.rseed, &rho))
                .ok_or_else(|| eyre!("invalid Orchard random seed"))?;
        let value = OrchardNoteValue::from_raw(self.value);
        Option::<note::Note>::from(note::Note::from_parts(address, value, rho, rseed))
            .ok_or_else(|| eyre!("invalid Orchard note parts"))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountWitness {
    pub ufvk: Vec<u8>,
    pub sapling_notes: Vec<SaplingNoteRecord>,
    pub orchard_notes: Vec<OrchardNoteRecord>,
}

impl AccountWitness {
    pub fn decode_ufvk(&self) -> Result<UnifiedFullViewingKey> {
        // Convert Vec<u8> to &str for decoding
        let ufvk_str = std::str::from_utf8(&self.ufvk)
            .map_err(|e| eyre!("UFVK bytes are not valid UTF-8: {e:?}"))?;
        UnifiedFullViewingKey::decode(&MAIN_NETWORK, ufvk_str)
            .map_err(|e| eyre!("failed to decode UFVK for account: {e:?}"))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HoldingsWitness {
    pub accounts: Vec<AccountWitness>,
    /// Nonce to force new job hash when resubmitting to monerochan network prover
    #[serde(default)]
    pub nonce: u64,
    /// Actual roots from the wallet/snapshot used to create witnesses
    /// This is the authoritative data that should be used for verification
    #[serde(default)]
    pub actual_snapshot: Option<SnapshotMetadata>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransparentShieldingTxRecord {
    pub txid: [u8; 32],
    pub branch_id: u32,
    pub raw_tx: Vec<u8>,
    pub inputs: Vec<TransparentShieldingInputRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TransparentShieldingInputRecord {
    pub index: u32,
    pub prev_txid: [u8; 32],
    pub prev_index: u32,
    pub value: u64,
    pub script_pubkey: Vec<u8>,
    pub commitment: [u8; 32],
    pub utxo_proof: Option<InclusionProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedHoldings {
    pub total_zatoshis: u64,
    pub alternate_nullifiers: Vec<[u8; 32]>,
    pub transparent_utxo_commitments: Vec<[u8; 32]>,
}

/// Merkle inclusion proof for a leaf in a sorted tree
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InclusionProof {
    pub leaf_hash: [u8; 32],
    pub leaf_index: u64,
    pub proof_path: Vec<[u8; 32]>,
    pub root: [u8; 32],
}

/// Merkle exclusion proof showing a value is NOT in a sorted tree
/// Uses gap-based approach with predecessor and successor
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExclusionProof {
    pub target: [u8; 32],
    /// Predecessor: largest value < target (None if target is smallest)
    pub predecessor: Option<ProofElement>,
    /// Successor: smallest value > target (None if target is largest)
    pub successor: Option<ProofElement>,
    pub root: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProofElement {
    pub hash: [u8; 32],
    pub index: u64,
    pub proof_path: Vec<[u8; 32]>,
}

/// Metadata about a snapshot at a specific height
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub height: u32,
    pub utxo_root: [u8; 32],
    pub nullifier_root: [u8; 32],
    pub utxo_count: u64,
    pub nullifier_count: u64,
    pub sapling_root: [u8; 32],
    pub orchard_root: [u8; 32],
    pub sapling_count: u64,
    pub orchard_count: u64,
}
