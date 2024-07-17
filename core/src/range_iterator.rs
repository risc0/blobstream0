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

use blobstream0_primitives::{LightBlockProveData, DEFAULT_PROVER_OPTS};
use tendermint_light_client_verifier::{types::LightBlock, ProdVerifier, Verdict};

/// Iterates over all blocks, yielding inputs for the light blocks to prove with the max number
/// of skipped blocks within the trust threshold.
pub struct LightBlockRangeIterator<'a> {
    pub trusted_block: &'a LightBlock,
    pub blocks: &'a [LightBlock],
}

impl LightBlockRangeIterator<'_> {
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

fn validator_stake_overlap(trusted: &LightBlock, target: &LightBlock) -> bool {
    let vp = ProdVerifier::default();
    let trusted_state = trusted.as_trusted_state();
    let target_state = target.as_untrusted_state();
    let verdict =
        vp.verify_commit_against_trusted(&target_state, &trusted_state, &DEFAULT_PROVER_OPTS);
    matches!(verdict, Verdict::Success)
}
