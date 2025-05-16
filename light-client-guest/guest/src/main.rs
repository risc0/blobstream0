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

use alloy_sol_types::SolValue;
use blobstream0_primitives::proto::{TrustedLightBlock, UntrustedLightBlock};
use blobstream0_primitives::{build_merkle_root, expect_block_hash, light_client_verify};
use blobstream0_primitives::{generate_bitmap, RangeCommitment};
use risc0_zkvm::guest::env;
use tendermint_light_client_verifier::{types::Header, Verdict};
use tendermint_proto::Protobuf;

fn main() {
    // Deserialize inputs from host.
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
    // Assert all bytes have been read, as a sanity check.
    assert!(cursor.is_empty());

    // Generate validator bitmap of intersection of trusted and untrusted block signatures.
    let validator_bitmap = generate_bitmap(&trusted_block, &untrusted_block);

    // Build merkle root, while also verifying hash links between all blocks.
    let merkle_root = build_merkle_root(&trusted_block, &interval_headers, &untrusted_block);

    // Verify the light client transition to untrusted block
    let verdict = light_client_verify(&trusted_block, &untrusted_block);
    assert!(
        matches!(verdict, Verdict::Success),
        "validation failed, {:?}",
        verdict
    );

    // Commit ABI encoded data to journal to use in contract.
    let commit = RangeCommitment {
        trustedHeaderHash: expect_block_hash(trusted_block.signed_header.header()).into(),
        newHeight: untrusted_block.signed_header.header.height.value(),
        newHeaderHash: expect_block_hash(untrusted_block.signed_header.header()).into(),
        merkleRoot: merkle_root.into(),
        validatorBitmap: validator_bitmap.into(),
    };
    env::commit_slice(commit.abi_encode().as_slice());
}
