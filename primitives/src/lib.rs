// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use abi::IBlobstream::DataRootTuple;
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
    #[cfg(target_os = "zkvm")]
    sol!(
        #[derive(Debug)]
        IBlobstream,
        "../contracts/artifacts/Blobstream0.json"
    );
}
pub use abi::IBlobstream;

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
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid proof for given root")]
pub struct VerifyProofError;
