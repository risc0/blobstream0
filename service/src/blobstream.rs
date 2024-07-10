use std::{sync::Arc, time::Duration};

use alloy::{network::Network, providers::Provider, transports::Transport};
use host::{post_batch, prove_block_range};
use risc0_tm_core::IBlobstream::IBlobstreamInstance;
use tendermint_rpc::{Client, HttpClient};

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
    pub async fn spawn(&self) {
        loop {
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

            let (height, hash, tm_height) = tokio::join!(height_task, hash_task, tm_height_task);

            // TODO handle errors gracefully
            let height = height.unwrap().unwrap()._0;
            // TODO check this hash against tm node as sanity check
            let hash = hash.unwrap().unwrap()._0;
            let tm_height = tm_height.unwrap().unwrap();

            // TODO can prove proactively, this is very basic impl
            let block_target = height + self.batch_size;
            if block_target > tm_height.value() {
                // Cannot create a batch yet, wait until ready
                tokio::time::sleep(Duration::from_secs(15 * (block_target - tm_height.value())))
                    .await;
                continue;
            }

            // TODO gracefully handle errors
            let receipt = prove_block_range(&self.tm_client, height..block_target)
                .await
                .unwrap();
            post_batch(&self.contract, &receipt).await.unwrap();

            // TODO ensure height is updated
        }
    }
}
