use alloy_sol_types::SolValue;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tendermint::merkle::simple_hash_from_byte_vectors;

mod abi {
    use alloy_sol_types::sol;

    // TODO have this be built at compile time rather than manually
    #[cfg(not(target_os = "zkvm"))]
    sol!(
        #[derive(Debug)]
        #[sol(rpc)]
        IBlobstream,
        "../contracts/artifacts/Blobstream0.json"
    );
    // NOTE: These are likely redundant, but we cannot use the rpc codegen in zkvm
    // TODO clean this up, likely best to just conditionally apply annotation
    sol!("../contracts/src/RangeCommitment.sol");
    sol!("../contracts/lib/blobstream-contracts/src/DataRootTuple.sol");
}
pub use abi::DataRootTuple;
#[cfg(not(target_os = "zkvm"))]
pub use abi::IBlobstream;
pub use abi::RangeCommitment;

#[derive(Debug, Serialize, Deserialize)]
pub struct LightClientCommit {
    #[serde(with = "serde_bytes")]
    pub trusted_block_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub next_block_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub next_data_root: [u8; 32],
    pub next_block_height: u64,
}

/// Type for the leaves in the [MerkleTree].
pub type MerkleHash = [u8; 32];

/// Merkle tree implementation for blobstream header proof and validation.
#[derive(Default)]
pub struct MerkleTree {
    inner: Vec<Vec<u8>>,
}

impl MerkleTree {
    pub fn push(&mut self, element: &DataRootTuple) {
        self.push_raw(element.abi_encode());
    }

    pub fn push_raw(&mut self, bytes: Vec<u8>) {
        self.inner.push(bytes)
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
        simple_hash_from_byte_vectors::<Sha256>(&self.inner)
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
