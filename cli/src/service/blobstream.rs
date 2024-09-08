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

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy::{network::Network, primitives::FixedBytes, providers::Provider, transports::Transport};
use blobstream0_core::{post_batch, prove_block_range};
use blobstream0_primitives::IBlobstream::IBlobstreamInstance;
use rand::Rng;
use tendermint_rpc::{Client, HttpClient};
use tokio::task::JoinError;

macro_rules! log_failure {
    ($res:expr, $($arg:tt)*) => {{
        let res = $res;
        if let Err(e) = &res {
            tracing::warn!(
                target: "blobstream0::service",
                $($arg)*,
                e,
            );
        }
        res
    }}
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

    /// Fetches the current state, generates a proof for a new merkle root, publishes that root.
    async fn progress_contract_state(&self) -> anyhow::Result<()> {
        // Poll Tendermint state until enough blocks to generate proof.
        let (trusted_height, untrusted_height) = loop {
            let BlobstreamState {
                eth_verified_height,
                tm_height,
                // TODO check this hash against tm node as sanity check
                eth_verified_hash: _,
            } = log_failure!(
                self.fetch_current_state().await?,
                "failed to fetch current state: {}"
            )?;
            tracing::info!(
                target: "blobstream0::service",
                "Contract height: {eth_verified_height}, tendermint height: {tm_height}"
            );

            let trusted_height = eth_verified_height + 1;
            let untrusted_height = trusted_height + self.batch_size;
            if untrusted_height > tm_height {
                // Underestimating wait time, it's cheap to fetch current state.
                let wait_time = 10 + (3 * (untrusted_height - tm_height));
                tracing::info!(
                    target: "blobstream0::service",
                    "Not enough tendermint blocks to create batch, waiting {} seconds",
                    wait_time
                );
                tokio::time::sleep(Duration::from_secs(wait_time)).await;
            }

            break (trusted_height, untrusted_height);
        };

        let receipt = log_failure!(
            prove_block_range(self.tm_client.clone(), trusted_height..untrusted_height).await,
            "failed to prove block range: {}"
        )?;
        log_failure!(
            post_batch(&self.contract, &receipt).await,
            "failed to post batch: {}"
        )?;

        // TODO ensure height is updated as a sanity check
        Ok(())
    }

    /// Spawn blobstream service, which will run indefinitely until a fatal error when awaited.
    pub async fn spawn(&self) -> anyhow::Result<()> {
        loop {
            exponential_backoff(|| async { Ok(self.progress_contract_state().await?) }).await?;
        }
    }
}

/// Basic exponential backoff implementation. Async retry libraries caused issues.
async fn exponential_backoff<F, Fut, T>(mut operation: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let start_time = Instant::now();
    let initial_interval = Duration::from_secs(1);
    let max_interval = Duration::from_secs(2 * 60 * 60); // 2 hours
    let timeout = Duration::from_secs(2 * 24 * 60 * 60); // 2 days

    let mut current_interval = initial_interval;
    let mut rng = rand::thread_rng();

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if start_time.elapsed() >= timeout {
                    return Err(e);
                }

                let jitter = rng.gen_range(0..=1000);
                let sleep_duration = current_interval + Duration::from_millis(jitter);

                tokio::time::sleep(sleep_duration).await;

                current_interval = std::cmp::min(current_interval * 2, max_interval);
            }
        }
    }
}

struct BlobstreamState {
    eth_verified_height: u64,
    #[allow(dead_code)]
    eth_verified_hash: FixedBytes<32>,
    tm_height: u64,
}
