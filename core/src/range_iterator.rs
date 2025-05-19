// Copyright 2025 RISC Zero, Inc.
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

use blobstream0_primitives::{
    light_client_verify,
    proto::{TrustedLightBlock, UntrustedLightBlock},
    LightBlockProveData,
};
use tendermint_light_client_verifier::{types::Header, Verdict};
use tendermint_rpc::HttpClient;

use crate::{fetch_untrusted_light_block, fetch_validators};

/// Iterates over all blocks, yielding inputs for the light blocks to prove with the max number
/// of skipped blocks within the trust threshold.
pub struct LightBlockRangeIterator<'a> {
    pub client: &'a HttpClient,
    pub trusted_block: TrustedLightBlock,
    pub blocks: &'a [Header],
}

impl LightBlockRangeIterator<'_> {
    async fn last_valid_idx(&self) -> anyhow::Result<Option<(usize, UntrustedLightBlock)>> {
        // Short circuit if entire range can be proven.
        {
            let Some(last) = self.blocks.last() else {
                return Ok(None);
            };
            let untrusted = fetch_untrusted_light_block(self.client, last.height).await?;
            if validator_stake_overlap(&self.trusted_block, &untrusted) {
                return Ok(Some((self.blocks.len() - 1, untrusted)));
            }
        }

        // Fallback to binary search.
        // Note: this will find the largest possible skip, to avoid unnecessary proving in the guest
        let (mut left, mut right) = (0, self.blocks.len() - 1);
        let mut result = None;

        while left <= right {
            let mid = left + (right - left) / 2;
            let block = &self.blocks[mid];

            let untrusted = fetch_untrusted_light_block(self.client, block.height).await?;
            if validator_stake_overlap(&self.trusted_block, &untrusted) {
                // Found valid transition, update max and search upper half for a larger skip
                result = Some((mid, untrusted));
                left = mid + 1;
            } else {
                right = mid - 1;
            }
        }

        Ok(result)
    }
}

impl LightBlockRangeIterator<'_> {
    pub(crate) async fn next_range(&mut self) -> anyhow::Result<Option<LightBlockProveData>> {
        let Some((block_idx, untrusted_block)) = self.last_valid_idx().await? else {
            return Ok(None);
        };
        let (prove_range, next_range) = self.blocks.split_at(block_idx + 1);
        let Some((target_block, header_blocks)) = prove_range.split_last() else {
            return Ok(None);
        };

        let target_height = target_block.height;
        let next_validators = fetch_validators(&self.client, target_height.increment()).await?;

        // Update iterator state to use target as trusted, and remove proven range of blocks
        let old_trusted_block = core::mem::replace(
            &mut self.trusted_block,
            TrustedLightBlock {
                signed_header: untrusted_block.signed_header.clone(),
                next_validators,
            },
        );
        self.blocks = next_range;

        Ok(Some(LightBlockProveData {
            trusted_block: old_trusted_block,
            interval_headers: header_blocks.to_vec(),
            untrusted_block,
        }))
    }
}

fn validator_stake_overlap(trusted: &TrustedLightBlock, target: &UntrustedLightBlock) -> bool {
    // Replicate same validation as done in the guest, to avoid any inconsistencies
    matches!(light_client_verify(trusted, target), Verdict::Success)
}
