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
//
// SPDX-License-Identifier: Apache-2.0

use crate::DEFAULT_PROVER_OPTS;
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use tendermint_light_client_verifier::{
    types::{Header, LightBlock},
    ProdVerifier, Verdict,
};

/// Inputs for light client block proving for Blobstream. Serialized as tuple for more compact form.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct LightBlockProveData {
    pub trusted_block: LightBlock,
    pub interval_headers: Vec<Header>,
    pub untrusted_block: LightBlock,
}

impl LightBlockProveData {
    /// Height of the block to prove to.
    pub fn target_height(&self) -> u64 {
        self.untrusted_block.signed_header.header.height.value()
    }

    /// Trusted height for the starting point of the proof.
    pub fn trusted_height(&self) -> u64 {
        self.trusted_block.signed_header.header.height.value()
    }
}

pub struct LightBlockRangeIterator<'a> {
    pub trusted_block: &'a LightBlock,
    pub blocks: &'a [LightBlock],
}

impl<'a> LightBlockRangeIterator<'a> {
    pub fn last_valid_idx(&self) -> Option<usize> {
        // Short circuit if entire range can be proven.
        {
            let last = self.blocks.last()?;
            if validator_stake_overlap(&self.trusted_block, last) {
                return Some(self.blocks.len() - 1);
            }
        }

        // Fallback to binary search.
        // Note: this will find the largest possible skip, to avoid unnecessary proving in the guest
        let (mut left, mut right) = (0, self.blocks.len() - 1);
        let mut result = None;

        while left <= right {
            let mid = left + (right - left) / 2;
            let block = &self.blocks[mid];

            if validator_stake_overlap(&self.trusted_block, block) {
                // Found valid transition, update max and search upper half for a larger skip
                result = Some(mid);
                left = mid + 1;
            } else {
                right = mid - 1;
            }
        }

        result
    }
}

fn validator_stake_overlap(trusted: &LightBlock, target: &LightBlock) -> bool {
    let vp = ProdVerifier::default();
    let trusted_state = trusted.as_trusted_state();
    let target_state = target.as_untrusted_state();
    let verdict =
        vp.verify_commit_against_trusted(&target_state, &trusted_state, &DEFAULT_PROVER_OPTS);
    matches!(verdict, Verdict::Success)
}

impl Iterator for LightBlockRangeIterator<'_> {
    // Note: this could be optimized to avoid clones/ownership, not worth the optimization for
    // 		 the host yet though.
    type Item = LightBlockProveData;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO double check the options can't be hit, likely irrecoverable in those cases
        let block_idx = self.last_valid_idx()?;
        let (prove_range, next_range) = self.blocks.split_at(block_idx + 1);
        let (target_block, header_blocks) = prove_range.split_last()?;
        let interval_headers = header_blocks
            .iter()
            .map(|h| h.signed_header.header.clone())
            .collect();

        let data = LightBlockProveData {
            trusted_block: self.trusted_block.clone(),
            interval_headers,
            untrusted_block: target_block.clone(),
        };

        // Update iterator state to use target as trusted, and remove proven range of blocks
        *self = LightBlockRangeIterator {
            trusted_block: target_block,
            blocks: next_range,
        };

        Some(data)
    }
}
