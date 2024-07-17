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

use alloy::{
    hex::FromHex,
    network::EthereumWallet,
    primitives::{hex, Address, FixedBytes},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::sol;
use blobstream0_core::prove_block_range;
use blobstream0_primitives::IBlobstream;
use clap::Parser;
use std::{path::PathBuf, sync::Arc};
use tendermint_rpc::HttpClient;
use tokio::fs;
use tracing_subscriber::fmt::format;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

// TODO elsewhere if keeping dev mode deploy through CLI
sol!(
    #[sol(rpc)]
    MockVerifier,
    // TODO probably not ideal to reference build directory, fine for now.
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);
sol!(
    #[sol(rpc)]
    RiscZeroGroth16Verifier,
    // TODO probably not ideal to reference build directory, fine for now.
    "../contracts/out/RiscZeroGroth16Verifier.sol/RiscZeroGroth16Verifier.json"
);

// Pulled from https://github.com/risc0/risc0-ethereum/blob/ebec385cc526adb9279c1af55d699c645ca6d694/contracts/src/groth16/ControlID.sol
const CONTROL_ID: [u8; 32] =
    hex!("a516a057c9fbf5629106300934d48e0e775d4230e41e503347cad96fcbde7e2e");
const BN254_CONTROL_ID: [u8; 32] =
    hex!("0eb6febcf06c5df079111be116f79bd8c7e85dc9448776ef9a59aaf2624ab551");

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

    /// Address of risc0 verifier to use (either mock or groth16)
    #[clap(long)]
    verifier_address: Option<String>,

    /// Trusted height for contract
    #[clap(long)]
    tm_height: u64,

    /// Trusted block hash for contract
    #[clap(long)]
    tm_block_hash: String,

    /// If deploying verifier, will it deploy the mock verifier
    #[clap(long)]
    dev: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .event_format(format().compact())
        .with_span_events(FmtSpan::CLOSE)
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

            let client = Arc::new(HttpClient::new(tendermint_rpc.as_str())?);

            let receipt = prove_block_range(client, start..end).await?;

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
                let deployed_address = if deploy.dev {
                    tracing::debug!("Deploying mock verifier");
                    MockVerifier::deploy(&provider, [0, 0, 0, 0].into())
                        .await?
                        .address()
                        .clone()
                } else {
                    tracing::debug!("Deploying groth16 verifier");
                    RiscZeroGroth16Verifier::deploy(
                        &provider,
                        CONTROL_ID.into(),
                        BN254_CONTROL_ID.into(),
                    )
                    .await?
                    .address()
                    .clone()
                };
                println!("deployed verifier to address: {}", deployed_address);
                deployed_address
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
