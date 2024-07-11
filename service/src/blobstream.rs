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

use std::{sync::Arc, time::Duration};

use alloy::{network::Network, primitives::FixedBytes, providers::Provider, transports::Transport};
use host::{post_batch, prove_block_range};
use risc0_tm_core::IBlobstream::IBlobstreamInstance;
use tendermint_rpc::{Client, HttpClient};
use tokio::task::JoinError;

macro_rules! handle_temporal_result {
    ($res:expr, $consecutive_failures:expr) => {
        match $res {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("failed to request current state: {}", e);
                $consecutive_failures += 1;
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                continue;
            }
        }
    };
}

pub(crate) struct BlobstreamService<T, P, N> {
    contract: Arc<IBlobstreamInstance<T, P, N>>,
    tm_client: Arc<HttpClient>,
    batch_size: u64,
}

impl<T, P, N> BlobstreamService<T, P, N> {
    pub fn new(
        contract: IBlobstreamInstance<T, P, N>,
        tm_client: HttpClient,
        batch_size: u64,
    ) -> Self {
        Self {
            contract: Arc::new(contract),
            tm_client: Arc::new(tm_client),
            batch_size,
        }
    }
}

impl<T, P, N> BlobstreamService<T, P, N>
where
    T: Transport + Clone,
    P: Provider<T, N> + 'static,
    N: Network,
{
    async fn fetch_current_state(&self) -> Result<anyhow::Result<BlobstreamState>, JoinError> {
        let contract = Arc::clone(&self.contract);
        let height_task = tokio::spawn(async move { contract.latestHeight().call().await });
        let contract = Arc::clone(&self.contract);
        let hash_task = tokio::spawn(async move { contract.latestBlockHash().call().await });
        let tm_client = Arc::clone(&self.tm_client);
        let tm_height_task = tokio::spawn(async move {
            tm_client
                .status()
                .await
                .map(|status| status.sync_info.latest_block_height)
        });

        let (height, hash, tm_height) = tokio::try_join!(height_task, hash_task, tm_height_task)?;

        let result = || {
            let height = height?._0;
            let eth_verified_hash = hash?._0;
            let tm_height = tm_height?.value();
            Ok(BlobstreamState {
                eth_verified_height: height,
                eth_verified_hash,
                tm_height,
            })
        };

        Ok(result())
    }

    /// Spawn blobstream service, which will run indefinitely until a fatal error when awaited.
    pub async fn spawn(&self) -> anyhow::Result<()> {
        let mut consecutive_failures = 0;
        while consecutive_failures < 5 {
            let BlobstreamState {
                eth_verified_height,
                tm_height,
                // TODO check this hash against tm node as sanity check
                eth_verified_hash: _, 
            } = handle_temporal_result!(self.fetch_current_state().await?, consecutive_failures);
            tracing::info!(
                "Contract height: {eth_verified_height}, tendermint height: {tm_height}"
            );

            // TODO can prove proactively, this is very basic impl
            let block_target = eth_verified_height + self.batch_size;
            if block_target > tm_height {
                let wait_time = 15 * (block_target - tm_height);
                tracing::info!(
                    "Not enough tendermint blocks to create batch, waiting {} seconds",
                    wait_time
                );
                // Cannot create a batch yet, wait until ready
                tokio::time::sleep(Duration::from_secs(wait_time)).await;
                continue;
            }

            let receipt = handle_temporal_result!(
                prove_block_range(&self.tm_client, eth_verified_height..block_target).await,
                consecutive_failures
            );
            handle_temporal_result!(
                post_batch(&self.contract, &receipt).await,
                consecutive_failures
            );

            consecutive_failures = 0;

            // TODO ensure height is updated
        }

        anyhow::bail!("Reached limit of consecutive errors");
    }
}

struct BlobstreamState {
    eth_verified_height: u64,
    #[allow(dead_code)]
    eth_verified_hash: FixedBytes<32>,
    tm_height: u64,
}
