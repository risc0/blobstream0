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

use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use blobstream0_primitives::proto::{TrustedLightBlock, UntrustedLightBlock};
use blobstream0_primitives::IBlobstream::{DataRootTuple, RangeCommitment};
use blobstream0_primitives::{MerkleTree, DEFAULT_PROVER_OPTS};
use core::time::Duration;
use risc0_zkvm::guest::env;
use std::iter;
use tendermint::Hash;
use tendermint_light_client_verifier::{types::Header, ProdVerifier, Verdict, Verifier};
use tendermint_proto::Protobuf;

fn build_merkle_root(
    trusted_block: &TrustedLightBlock,
    interval_headers: &[Header],
    untrusted_block: &UntrustedLightBlock,
) -> [u8; 32] {
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

// TODO move this to primitives if possible, and re-use in host checks.
fn light_client_verify(trusted_block: &TrustedLightBlock, untrusted_block: &UntrustedLightBlock) {
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
    let verdict = vp.verify_update_header(
        untrusted_state,
        trusted_state,
        &DEFAULT_PROVER_OPTS,
        verify_time.unwrap(),
    );

    assert!(
        matches!(verdict, Verdict::Success),
        "validation failed, {:?}",
        verdict
    );
}

fn main() {
    let mut len: u32 = 0;
    env::read_slice(core::slice::from_mut(&mut len));
    let mut buf = vec![0; len as usize];
    env::read_slice(&mut buf);

    let mut cursor = buf.as_slice();

    let trusted_block = TrustedLightBlock::decode_length_delimited(&mut cursor).unwrap();
    let untrusted_block = UntrustedLightBlock::decode_length_delimited(&mut cursor).unwrap();

    let num_headers = untrusted_block.signed_header.header.height.value()
        - trusted_block.signed_header.header.height.value()
        - 1;
    let mut interval_headers = Vec::with_capacity(num_headers.try_into().unwrap());
    for _ in 0..num_headers {
        let header: Header =
            Protobuf::<tendermint_proto::v0_37::types::Header>::decode_length_delimited(
                &mut cursor,
            )
            .unwrap();
        interval_headers.push(header);
    }
    // Assert all bytes have been read, as a sanity check
    assert!(cursor.is_empty());

    let merkle_root = build_merkle_root(&trusted_block, &interval_headers, &untrusted_block);

    // Verify the light client transition to untrusted block
    light_client_verify(&trusted_block, &untrusted_block);

    let commit = RangeCommitment {
        trustedHeaderHash: expect_block_hash(trusted_block.signed_header.header()).into(),
        newHeight: untrusted_block.signed_header.header.height.value(),
        newHeaderHash: expect_block_hash(untrusted_block.signed_header.header()).into(),
        merkleRoot: merkle_root.into(),
    };
    env::commit_slice(commit.abi_encode().as_slice());
}

fn expect_block_hash(block: &Header) -> [u8; 32] {
    let Hash::Sha256(first_block_hash) = block.hash() else {
        unreachable!("Header hash should always be a non empty sha256");
    };
    first_block_hash
}

fn expect_sha256_data_hash(header: &Header) -> [u8; 32] {
    let Some(Hash::Sha256(first_block_hash)) = header.data_hash else {
        unreachable!("Header data root should always be a non empty sha256");
    };
    first_block_hash
}
