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
use blobstream0_primitives::{
    IBlobstream::{DataRootTuple, RangeCommitment},
    LightClientCommit, MerkleTree,
};
use light_client_guest::TM_LIGHT_CLIENT_ID;
use risc0_zkvm::{guest::env, serde::from_slice};
use serde_bytes::ByteBuf;

/// Alias to represent the bytes from the journals being recursively proven.
type JournalBytes = ByteBuf;

fn main() {
    // Input the vector of proofs to batch.
    let input: Vec<JournalBytes> = env::read();

    let mut trusted_header_hash = None;
    let mut merkle_tree = MerkleTree::default();
    let mut last_verified: Option<LightClientCommit> = None;
    for journal in input {
        env::verify(TM_LIGHT_CLIENT_ID, &journal).unwrap();
        let commit: LightClientCommit = from_slice(&journal).unwrap();
        if let Some(prev_verified) = last_verified.as_ref() {
            // Assert that the previous block commitment equals the next.
            assert_eq!(&commit.trusted_block_hash, &prev_verified.next_block_hash);
        } else {
            trusted_header_hash = Some(commit.trusted_block_hash);
        }

        for (height, data_root) in &commit.data_roots {
            // TODO this is a bit inefficient, since we don't need to keep intermediate nodes in heap.
            //      ideally this just generates hash and drops any intermediate value. (minor opt)
            merkle_tree.push(&DataRootTuple {
                height: U256::from(*height),
                dataRoot: data_root.into(),
            });
        }

        // Set the most recently validated block, to validate the next against.
        last_verified = Some(commit);
    }

    let latest_block = last_verified.unwrap();
    let commit = RangeCommitment {
        trustedHeaderHash: trusted_header_hash
            .expect("must be at least one verified block")
            .into(),
        newHeight: latest_block.data_roots.last().unwrap().0,
        newHeaderHash: latest_block.next_block_hash.into(),
        merkleRoot: merkle_tree.root().into(),
    };
    env::commit_slice(commit.abi_encode().as_slice());
}
