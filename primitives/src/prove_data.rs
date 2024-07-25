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

use tendermint_light_client_verifier::types::Header;

use crate::proto::{TrustedLightBlock, UntrustedLightBlock};

/// Inputs for light client block proving for Blobstream. Serialized as tuple for more compact form.
#[derive(Debug)]
pub struct LightBlockProveData {
    pub trusted_block: TrustedLightBlock,
    pub interval_headers: Vec<Header>,
    pub untrusted_block: UntrustedLightBlock,
}

impl LightBlockProveData {
    /// Height of the block to prove to.
    pub fn untrusted_height(&self) -> u64 {
        self.untrusted_block.signed_header.header.height.value()
    }

    /// Trusted height for the starting point of the proof.
    pub fn trusted_height(&self) -> u64 {
        self.trusted_block.signed_header.header.height.value()
    }
}
