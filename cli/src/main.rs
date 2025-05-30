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

use alloy::{
    hex::FromHex,
    primitives::{hex, Address, FixedBytes},
    providers::ProviderBuilder,
};
use alloy_sol_types::{sol, SolCall};
use blobstream0_core::prove_block_range;
use blobstream0_primitives::IBlobstream;
use clap::Parser;
use dotenv::dotenv;
use std::{path::PathBuf, sync::Arc};
use tendermint_rpc::HttpClient;
use tokio::fs;
use tracing_subscriber::fmt::format;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[cfg(feature = "fireblocks")]
pub(crate) mod fireblocks;
mod service;

sol!(
    #[sol(rpc)]
    MockVerifier,
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);
sol!(
    #[sol(rpc)]
    RiscZeroGroth16Verifier,
    "../contracts/out/RiscZeroGroth16Verifier.sol/RiscZeroGroth16Verifier.json"
);
sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "../contracts/out/ERC1967Proxy.sol/ERC1967Proxy.json"
);

// Pulled from https://github.com/risc0/risc0-ethereum/blob/ebec385cc526adb9279c1af55d699c645ca6d694/contracts/src/groth16/ControlID.sol
const CONTROL_ID: [u8; 32] =
    hex!("8b6dcf11d463ac455361b41fb3ed053febb817491bdea00fdb340e45013b852e");
const BN254_CONTROL_ID: [u8; 32] =
    hex!("05a022e1db38457fb510bc347b30eb8f8cf3eda95587653d0eac19e1f10d164e");

#[cfg(feature = "fireblocks")]
macro_rules! setup_provider {
    ($cli:ident) => {{
        let fireblocks_address: Address = $cli.fireblocks_address.parse()?;
        let provider = ProviderBuilder::new()
            .fetch_chain_id()
            .filler(crate::fireblocks::FireblocksFiller {
                sender: fireblocks_address,
            })
            .connect_http($cli.eth_rpc.parse()?);
        (provider, fireblocks_address)
    }};
}

#[cfg(not(feature = "fireblocks"))]
macro_rules! setup_provider {
    ($cli:ident) => {{
        let signer: alloy::signers::local::PrivateKeySigner = $cli.private_key_hex.parse()?;
        let signer_address = signer.address();
        let provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(signer))
            .connect_http($cli.eth_rpc.parse()?);
        (provider, signer_address)
    }};
}

pub(crate) use setup_provider;

#[derive(Parser, Debug)]
#[command(name = "blobstream0-cli")]
#[command(bin_name = "blobstream0-cli")]
enum BlobstreamCli {
    Service(service::ServiceArgs),
    ProveRange(ProveRangeArgs),
    Deploy(DeployArgs),
    Upgrade(UpgradeArgs),
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
    #[clap(long, env)]
    tendermint_rpc: String,

    /// Output file path to write serialized receipt to
    #[clap(long, short)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct DeployArgs {
    /// The Ethereum RPC URL
    #[clap(long, env)]
    eth_rpc: String,

    #[cfg(feature = "fireblocks")]
    /// Fireblocks signer address.
    #[clap(long, env)]
    fireblocks_address: String,

    #[cfg(not(feature = "fireblocks"))]
    /// Hex encoded private key to use for deploying.
    #[clap(long, env)]
    private_key_hex: String,

    /// Hex encoded address of admin for upgrades. Will default to the private key address.
    #[clap(long, env)]
    admin_address: Option<String>,

    /// Address of risc0 verifier to use (either mock or groth16)
    #[clap(long, env)]
    verifier_address: Option<String>,

    /// Trusted height for contract
    #[clap(long, env)]
    tm_height: u64,

    /// Trusted block hash for contract
    #[clap(long, env)]
    tm_block_hash: String,

    /// Minimum batch size for head updates. The batch size must be larger than this value.
    #[clap(long, env)]
    min_batch_size: u64,

    /// If deploying verifier, will it deploy the mock verifier
    #[clap(long)]
    dev: bool,
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct UpgradeArgs {
    /// The Ethereum RPC URL
    #[clap(long, env)]
    eth_rpc: String,

    #[cfg(feature = "fireblocks")]
    /// Fireblocks signer address.
    #[clap(long, env)]
    fireblocks_address: String,

    #[cfg(not(feature = "fireblocks"))]
    /// Hex encoded private key to use for deploying.
    #[clap(long, env)]
    private_key_hex: String,

    /// Hex encoded address of admin for upgrades. Will default to the private key address.
    #[clap(long, env)]
    proxy_address: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

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
            let (provider, signer_address) = setup_provider!(deploy);
            let admin_address: Address = if let Some(address) = deploy.admin_address {
                address.parse()?
            } else {
                signer_address
            };

            let verifier_address: Address = if let Some(address) = deploy.verifier_address {
                address.parse()?
            } else {
                let deployed_address = if deploy.dev {
                    tracing::debug!(target: "blobstream0::cli", "Deploying mock verifier");
                    MockVerifier::deploy(&provider, [0, 0, 0, 0].into())
                        .await?
                        .address()
                        .clone()
                } else {
                    tracing::debug!(target: "blobstream0::cli", "Deploying groth16 verifier");
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
            let implementation = IBlobstream::deploy(&provider).await?;
            tracing::debug!(target: "blobstream0::cli", "Deployed implementation contract to {}", implementation.address());

            let proxy = ERC1967Proxy::deploy(
                &provider,
                implementation.address().clone(),
                IBlobstream::initializeCall {
                    _admin: admin_address,
                    _verifier: verifier_address,
                    _trustedHash: FixedBytes::<32>::from_hex(deploy.tm_block_hash)?,
                    _trustedHeight: deploy.tm_height,
                    _minBatchSize: deploy.min_batch_size,
                }
                .abi_encode()
                .into(),
            )
            .await?;
            tracing::debug!(target: "blobstream0::cli", "Deployed proxy contract");

            println!("deployed contract to address: {}", proxy.address());
        }
        BlobstreamCli::Upgrade(upgrade) => {
            let (provider, _) = setup_provider!(upgrade);

            let proxy_address: Address = upgrade.proxy_address.parse()?;
            println!("proxy address: {}", proxy_address);

            let implementation = IBlobstream::deploy(&provider).await?;
            tracing::debug!(target: "blobstream0::cli", "Deployed new implementation contract to {}", implementation.address());

            IBlobstream::new(proxy_address, provider.clone())
                .upgradeToAndCall(implementation.address().clone(), Default::default())
                .send()
                .await?
                .watch()
                .await?;
            tracing::debug!(target: "blobstream0::cli", "Upgraded proxy contract to new implementation");
            println!("Upgraded proxy contract to {}", implementation.address());
        }
        BlobstreamCli::Service(service) => service.start().await?,
    }

    Ok(())
}
