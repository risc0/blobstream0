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

use self::blobstream::BlobstreamService;
use super::setup_provider;
use alloy::{primitives::Address, providers::ProviderBuilder};
use blobstream0_primitives::IBlobstream;
use clap::Parser;
use tendermint_rpc::HttpClient;

mod blobstream;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct ServiceArgs {
    /// The Tendermint RPC URL
    #[clap(long, env)]
    tendermint_rpc: String,

    /// The Ethereum RPC URL
    #[clap(long, env)]
    eth_rpc: String,

    /// The deployed contract on Ethereum to reference
    #[clap(long, env)]
    eth_address: Address,

    #[cfg(feature = "fireblocks")]
    /// Fireblocks signer address.
    #[clap(long, env)]
    fireblocks_address: String,

    #[cfg(not(feature = "fireblocks"))]
    /// Hex encoded private key to use for deploying.
    #[clap(long, env)]
    private_key_hex: String,

    /// Number of blocks proved in each batch of block headers
    #[clap(long, env)]
    batch_size: u64,
}

impl ServiceArgs {
    pub(crate) async fn start(self) -> anyhow::Result<()> {
        let tm_client = HttpClient::new(self.tendermint_rpc.as_str())?;

        let (provider, _) = setup_provider!(self);

        let contract = IBlobstream::new(self.eth_address, provider);

        tracing::info!(target: "blobstream0::service", "Starting service");
        BlobstreamService::new(contract, tm_client, self.batch_size)
            .spawn()
            .await?;

        Ok(())
    }
}
