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

use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use tendermint_light_client_verifier::types::{Header, LightBlock};

/// Inputs for light client block proving for Blobstream. Serialized as tuple for more compact form.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct LightBlockProveData {
    pub trusted_block: LightBlock,
    pub interval_headers: Vec<Header>,
    pub untrusted_block: LightBlock,
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
