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

use blobstream0_primitives::LightClientCommit;
use core::time::Duration;
use risc0_zkvm::guest::env;
use tendermint::Hash;
use tendermint_light_client_verifier::{
    options::Options,
    types::{LightBlock, TrustThreshold},
    ProdVerifier, Verdict, Verifier,
};

fn main() {
    // TODO this probably wants to be protobuf
    let (light_block_previous, light_block_next): (LightBlock, LightBlock) =
        ciborium::from_reader(env::stdin()).unwrap();

    let vp = ProdVerifier::default();
    let opt = Options {
        // Trust threshold overriden to match security used by IBC default
        // See context https://github.com/informalsystems/hermes/issues/2876
        trust_threshold: TrustThreshold::TWO_THIRDS,
        // Two week trusting period (range of which blocks can be validated).
        trusting_period: Duration::from_secs(1_209_600),
        clock_drift: Default::default(),
    };

    let trusted_state = light_block_previous.as_trusted_state();
    let untrusted_state = light_block_next.as_untrusted_state();

    // Assert that the block is the next block after the trusted state.
    // NOTE: This is only necessary for blobstream to ensure that there are no skipped blocks
    //       in the batch proof. Otherwise not necessary.
    assert_eq!(trusted_state.height.increment(), untrusted_state.height());

    // Check the next_validators hash, as verify_update_header leaves it for caller to check.
    assert_eq!(
        trusted_state.next_validators.hash(),
        trusted_state.next_validators_hash
    );

    // Assert that next validators is provided, such that verify will check it.
    // Note: this is a bit redundant, given converting from LightBlock will always be Some,
    // but this is to be sure the check is always done, even if refactored.
    assert!(untrusted_state.next_validators.is_some());

    // This verify time picked pretty arbitrarily, need to be after header time and within
    // trusting window.
    let verify_time = light_block_next.time() + Duration::from_secs(1);
    let verdict =
        vp.verify_update_header(untrusted_state, trusted_state, &opt, verify_time.unwrap());

    assert!(
        matches!(verdict, Verdict::Success),
        "validation failed, {:?}",
        verdict
    );

    // TODO also mixing serialization with using default, resolve with above
    env::commit(&LightClientCommit {
        // TODO also committing block hashes, under the assumption that verifying those is more secure than
        //      verifying just the data roots. This might not be necessary.
        trusted_block_hash: expect_block_hash(&light_block_previous),
        next_block_hash: expect_block_hash(&light_block_next),
        next_data_root: expect_sha256_data_hash(&light_block_next),
        next_block_height: light_block_next.height().value(),
    });
}

fn expect_block_hash(block: &LightBlock) -> [u8; 32] {
    let Hash::Sha256(first_block_hash) = block.signed_header.header().hash() else {
        unreachable!("Header hash should always be a non empty sha256");
    };
    first_block_hash
}

fn expect_sha256_data_hash(block: &LightBlock) -> [u8; 32] {
    let Some(Hash::Sha256(first_block_hash)) = block.signed_header.header().data_hash else {
        unreachable!("Header data root should always be a non empty sha256");
    };
    first_block_hash
}
