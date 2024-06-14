use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};
use tiny_merkle::{
    proof::{MerkleProof as TinyMerkleProof, Position},
    MerkleTree as TinyMerkleTree,
};

#[derive(Serialize, Deserialize)]
pub struct LightClientCommit {
    pub first_block_hash: [u8; 32],
    pub next_block_hash: [u8; 32],
}

/// Type for the leaves in the [MerkleTree].
pub type MerkleHash = [u8; 32];

/// Proof generated for a leaf of a [MerkleTree].
pub type MerkleProof = TinyMerkleProof<KeccakHasher>;

// TODO avoid exposing this once below fixed
#[derive(Clone, Debug)]
pub struct KeccakHasher;
impl tiny_merkle::Hasher for KeccakHasher {
    type Hash = MerkleHash;

    fn hash(value: &[u8]) -> Self::Hash {
        keccak256(value)
    }
}

fn keccak256(data: &[u8]) -> MerkleHash {
    let mut hasher = Keccak::v256();
    let mut hash = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut hash);
    hash
}

/// Merkle tree implementation for blobstream header proof and validation.
#[derive(Debug)]
pub struct MerkleTree {
    inner: TinyMerkleTree<KeccakHasher>,
}

impl MerkleTree {
    /// Construct new merkle tree from all leaves.
    pub fn from_leaves(leaves: impl IntoIterator<Item = MerkleHash>) -> Self {
        Self {
            inner: TinyMerkleTree::from_leaves(leaves, None),
        }
    }

    /// Returns merkle root of tree.
    pub fn root(&self) -> MerkleHash {
        self.inner.root()
    }

    // TODO this return should be friendly to send to Ethereum.
    /// Generate merkle proof, which can be verified with [MerkleTree::verify_proof].
    pub fn generate_proof(&self, leaf: &MerkleHash) -> Option<TinyMerkleProof<KeccakHasher>> {
        self.inner.proof(leaf)
    }

    /// Verify generated proof created from [MerkleTree::generate_proof].
    ///
    /// Errors if the calculated root does not match the one passed in.
    pub fn verify_proof(
        leaf: MerkleHash,
        root: &MerkleHash,
        proof: &MerkleProof,
    ) -> Result<(), VerifyProofError> {
        let mut hash = leaf;
        let mut combine_buffer = [0u8; 64];
        for p in proof.proofs.iter() {
            if p.position == Position::Left {
                combine_hashes(&p.data, &hash, &mut combine_buffer);
                hash = <KeccakHasher as tiny_merkle::Hasher>::hash(combine_buffer.as_ref());
            } else {
                combine_hashes(&hash, &p.data, &mut combine_buffer);
                hash = <KeccakHasher as tiny_merkle::Hasher>::hash(combine_buffer.as_ref());
            }
        }

        if hash == root.as_ref() {
            Ok(())
        } else {
            Err(VerifyProofError)
        }
    }
}

fn combine_hashes(a: &MerkleHash, b: &MerkleHash, buffer: &mut [u8; 64]) {
    buffer[..32].copy_from_slice(a);
    buffer[32..].copy_from_slice(b);
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid proof for given root")]
pub struct VerifyProofError;
