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

// TODO move to separate CLI crate.

use std::path::PathBuf;

use alloy::{
    hex::FromHex,
    network::EthereumWallet,
    primitives::{Address, FixedBytes},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::sol;
use clap::Parser;
use host::prove_block_range;
use risc0_tm_core::IBlobstream;
use tendermint_rpc::HttpClient;
use tokio::fs;
use tracing_subscriber::EnvFilter;

// TODO elsewhere if keeping dev mode deploy through CLI
sol!(
    #[sol(rpc)]
    MockVerifier,
    // TODO probably not ideal to reference build directory, fine for now.
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);

#[derive(Parser, Debug)]
#[command(name = "blobstream0-cli")]
#[command(bin_name = "blobstream0-cli")]
enum BlobstreamCli {
    ProveRange(ProveRangeArgs),
    Deploy(DeployArgs),
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveRangeArgs {
    /// The start height
    #[clap(long)]
    start: u64,

    /// The end height of the batch
    #[clap(long)]
    end: u64,

    /// The Tendermint RPC URL
    #[clap(long)]
    tendermint_rpc: String,

    /// Output file path to write serialized receipt to
    #[clap(long, short)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct DeployArgs {
    /// The Ethereum RPC URL
    #[clap(long)]
    eth_rpc: String,

    /// Hex encoded private key to use for submitting proofs to Ethereum
    #[clap(long)]
    private_key_hex: String,

    #[clap(long)]
    verifier_address: Option<String>,

    #[clap(long)]
    tm_height: u64,

    #[clap(long)]
    tm_block_hash: String,

    #[clap(long)]
    dev: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    match BlobstreamCli::parse() {
        BlobstreamCli::ProveRange(range) => {
            let ProveRangeArgs {
                start,
                end,
                tendermint_rpc,
                out,
            } = range;

            let client = HttpClient::new(tendermint_rpc.as_str())?;

            let receipt = prove_block_range(&client, start..end).await?;

            fs::write(out, bincode::serialize(&receipt)?).await?;
        }
        BlobstreamCli::Deploy(deploy) => {
            let signer: PrivateKeySigner = deploy.private_key_hex.parse()?;
            let wallet = EthereumWallet::from(signer);

            let provider = ProviderBuilder::new()
                .with_recommended_fillers()
                .wallet(wallet)
                .on_http(deploy.eth_rpc.parse()?);
            let verifier_address: Address = if let Some(address) = deploy.verifier_address {
                address.parse()?
            } else {
                if deploy.dev {
                    MockVerifier::deploy(&provider, [0, 0, 0, 0].into())
                        .await?
                        .address()
                        .clone()
                } else {
                    unimplemented!("Cannot deploy groth16 verifier yet")
                }
            };

            // Deploy the contract.
            let contract = IBlobstream::deploy(
                &provider,
                verifier_address,
                FixedBytes::<32>::from_hex(deploy.tm_block_hash)?,
                deploy.tm_height,
            )
            .await?;

            println!("deployed contract to address: {}", contract.address());
        }
    }

    Ok(())
}
