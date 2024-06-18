use alloy_sol_types::SolValue;
use nmt_rs::{
    simple_merkle::{db::MemDb, proof::Proof, tree::MerkleTree as SimpleMerkleTree},
    TmSha2Hasher,
};
use serde::{Deserialize, Serialize};

mod abi {
    use alloy_sol_types::sol;

    #[cfg(not(target_os = "zkvm"))]
    sol!(
        #[sol(rpc)]
        IBlobstream,
        "../contracts/artifacts/Blobstream0.json"
    );
    sol!("../contracts/lib/blobstream-contracts/src/DataRootTuple.sol");
}
pub use abi::DataRootTuple;
#[cfg(not(target_os = "zkvm"))]
pub use abi::IBlobstream;

#[derive(Serialize, Deserialize)]
pub struct LightClientCommit {
    #[serde(with = "serde_bytes")]
    pub first_data_root: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub next_data_root: [u8; 32],
    pub next_block_height: u64,
}

/// Type for the leaves in the [MerkleTree].
pub type MerkleHash = [u8; 32];

/// Proof generated for a leaf of a [MerkleTree].
pub type MerkleProof = Proof<TmSha2Hasher>;

/// Merkle tree implementation for blobstream header proof and validation.
pub struct MerkleTree {
    inner: SimpleMerkleTree<MemDb<MerkleHash>, TmSha2Hasher>,
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self {
            inner: SimpleMerkleTree::new(),
        }
    }
}

impl MerkleTree {
    pub fn push(&mut self, element: &DataRootTuple) {
        self.push_raw(&element.abi_encode());
    }

    pub fn push_raw(&mut self, bytes: &[u8]) {
        // TODO to match Celestia, this has to encode the height with the hash.
        self.inner.push_raw_leaf(bytes)
    }

    /// Construct new merkle tree from all leaves.
    pub fn from_leaves<'a>(leaves: impl IntoIterator<Item = &'a DataRootTuple>) -> Self {
        let mut s = Self::default();
        for leaf in leaves {
            s.push(leaf);
        }
        s
    }

    /// Returns merkle root of tree.
    pub fn root(&mut self) -> MerkleHash {
        self.inner.root()
    }

    pub fn generate_proof(&mut self, index: usize) -> Proof<TmSha2Hasher> {
        self.inner.build_range_proof(index..index + 1)
    }

    // /// Verify generated proof created from [MerkleTree::generate_proof].
    // ///
    // /// Errors if the calculated root does not match the one passed in.
    // pub fn verify_proof(
    //     leaf: MerkleHash,
    //     root: &MerkleHash,
    //     proof: &MerkleProof,
    // ) -> Result<(), VerifyProofError> {
    //     let mut hash = leaf;
    //     let mut combine_buffer = [0u8; 64];
    //     for p in proof.proofs.iter() {
    //         if p.position == Position::Left {
    //             combine_hashes(&p.data, &hash, &mut combine_buffer);
    //             hash = <Sha2Hasher as tiny_merkle::Hasher>::hash(combine_buffer.as_ref());
    //         } else {
    //             combine_hashes(&hash, &p.data, &mut combine_buffer);
    //             hash = <Sha2Hasher as tiny_merkle::Hasher>::hash(combine_buffer.as_ref());
    //         }
    //     }

    //     if hash == root.as_ref() {
    //         Ok(())
    //     } else {
    //         Err(VerifyProofError)
    //     }
    // }
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid proof for given root")]
pub struct VerifyProofError;
