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

use blobstream0_primitives::proto::{TrustedLightBlock, UntrustedLightBlock};
use blobstream0_primitives::{LightClientCommit, DEFAULT_PROVER_OPTS};
use core::time::Duration;
use risc0_zkvm::guest::env;
use std::iter;
use tendermint::Hash;
use tendermint_light_client_verifier::{types::Header, ProdVerifier, Verdict, Verifier};
use tendermint_proto::Protobuf;

fn collect_data_roots(
    trusted_block: &TrustedLightBlock,
    interval_headers: &[Header],
    untrusted_block: &UntrustedLightBlock,
) -> Vec<(u64, [u8; 32])> {
    let trusted_header = trusted_block.signed_header.header();
    let untrusted_header = untrusted_block.signed_header.header();
    let mut previous = trusted_header;
    let mut range_data_roots = Vec::with_capacity(interval_headers.len() + 1);
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
        range_data_roots.push((header.height.value(), expect_sha256_data_hash(header)));
    }

    range_data_roots
}

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
    let len: u32 = env::read();
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

    let data_roots = collect_data_roots(&trusted_block, &interval_headers, &untrusted_block);

    // Verify the light client transition to untrusted block
    light_client_verify(&trusted_block, &untrusted_block);

    // TODO possibly avoid mixing serialization https://github.com/risc0/blobstream0/issues/1
    env::commit(&LightClientCommit {
        trusted_block_hash: expect_block_hash(&trusted_block.signed_header.header()),
        next_block_hash: expect_block_hash(&untrusted_block.signed_header.header()),
        data_roots,
    });
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
