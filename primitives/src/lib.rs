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
use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use proto::{TrustedLightBlock, UntrustedLightBlock};
use sha2::Sha256;
use std::iter;
use std::time::Duration;
use tendermint::merkle::simple_hash_from_byte_vectors;
use tendermint::Hash;
use tendermint_light_client_verifier::types::{Header, TrustThreshold};
use tendermint_light_client_verifier::{options::Options, ProdVerifier, Verdict, Verifier};

pub mod proto;

mod abi {
    use alloy_sol_types::sol;

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

mod prove_data;
pub use prove_data::LightBlockProveData;

/// Default options for validating Tendermint light client block transitions.
const DEFAULT_PROVER_OPTS: Options = Options {
    // Trust threshold overriden to match security used by IBC default
    // See context https://github.com/informalsystems/hermes/issues/2876
    trust_threshold: TrustThreshold::TWO_THIRDS,
    // Two week trusting period (range of which blocks can be validated).
    trusting_period: Duration::from_secs(1_209_600),
    clock_drift: Duration::from_secs(0),
};

/// Type for the leaves in the [MerkleTree].
pub type MerkleHash = [u8; 32];

/// Merkle tree implementation for blobstream header proof and validation.
#[derive(Default)]
struct MerkleTree {
    inner: Vec<Vec<u8>>,
}

impl MerkleTree {
    /// Pushes the encoded [DataRootTuple] to the merkle tree.
    pub fn push(&mut self, element: &DataRootTuple) {
        self.inner.push(element.abi_encode());
    }

    /// Computes and returns the merkle root of tree.
    pub fn root(&mut self) -> MerkleHash {
        simple_hash_from_byte_vectors::<Sha256>(&self.inner)
    }
}

/// Calculates merkle root of all new blocks proven. This includes the untrusted header and all
/// interval headers since the trusted block.
pub fn build_merkle_root(
    trusted_block: &TrustedLightBlock,
    interval_headers: &[Header],
    untrusted_block: &UntrustedLightBlock,
) -> MerkleHash {
    let mut merkle_tree = MerkleTree::default();

    let trusted_header = trusted_block.signed_header.header();
    let untrusted_header = untrusted_block.signed_header.header();
    let mut previous = trusted_header;
    for header in interval_headers.iter().chain(iter::once(untrusted_header)) {
        // Check hash links between blocks
        assert_eq!(
            header
                .last_block_id
                .expect("Header must hash link to previous block")
                .hash,
            previous.hash()
        );
        previous = header;

        // Push data root of checked header.
        merkle_tree.push(&DataRootTuple {
            height: U256::from(header.height.value()),
            dataRoot: expect_sha256_data_hash(header).into(),
        });
    }

    merkle_tree.root()
}

/// Verify light client transition from trusted block to untrusted.
pub fn light_client_verify(
    trusted_block: &TrustedLightBlock,
    untrusted_block: &UntrustedLightBlock,
) -> Verdict {
    let vp = ProdVerifier::default();

    let trusted_state = trusted_block.as_trusted_state();
    let untrusted_state = untrusted_block.as_untrusted_state();

    // Check the next_validators hash, as verify_update_header leaves it for caller to check.
    assert_eq!(
        trusted_state.next_validators.hash(),
        trusted_state.next_validators_hash
    );

    // This verify time picked pretty arbitrarily, need to be after header time and within
    // trusting window.
    let verify_time = untrusted_block.signed_header.header().time + Duration::from_secs(1);
    vp.verify_update_header(
        untrusted_state,
        trusted_state,
        &DEFAULT_PROVER_OPTS,
        verify_time.unwrap(),
    )
}

/// Convenience function to pull the block hash data, assuming a Sha256 hash.
pub fn expect_block_hash(block: &Header) -> [u8; 32] {
    let Hash::Sha256(hash) = block.hash() else {
        unreachable!("Header hash should always be a non empty sha256");
    };
    hash
}

/// Convenience function to pull the header's data hash, assuming a Sha256 hash.
fn expect_sha256_data_hash(header: &Header) -> [u8; 32] {
    let Some(Hash::Sha256(hash)) = header.data_hash else {
        unreachable!("Header data root should always be a non empty sha256");
    };
    hash
}
