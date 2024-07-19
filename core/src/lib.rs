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

use alloy::{network::Network, primitives::TxHash, providers::Provider, transports::Transport};
use alloy_sol_types::SolValue;
use anyhow::Context;
use batch_guest::BATCH_GUEST_ELF;
use blobstream0_primitives::{
    IBlobstream::{IBlobstreamInstance, RangeCommitment},
    LightBlockProveData, LightClientCommit,
};
use light_client_guest::TM_LIGHT_CLIENT_ELF;
use risc0_ethereum_contracts::groth16;
use risc0_zkvm::{default_prover, is_dev_mode, sha::Digestible, ExecutorEnv, ProverOpts, Receipt};
use serde_bytes::ByteBuf;
use std::{ops::Range, sync::Arc};
use tendermint::{block::Height, node::Id, validator::Set};
use tendermint_light_client_verifier::types::LightBlock;
use tendermint_rpc::{Client, HttpClient, Paging};
use tokio::{sync::Semaphore, task::JoinHandle};
use tracing::{instrument, Level};

mod range_iterator;
use range_iterator::LightBlockRangeIterator;

async fn fetch_light_block(
    client: &HttpClient,
    block_height: Height,
) -> anyhow::Result<LightBlock> {
    let commit_response = client.commit(block_height).await?;
    let signed_header = commit_response.signed_header;
    let height = signed_header.header.height;

    // Note: This currently needs to use Paging::All or the hash mismatches.
    let validator_response = client.validators(height, Paging::All).await?;

    let validators = Set::new(validator_response.validators, None);

    let next_validator_response = client.validators(height.increment(), Paging::All).await?;
    let next_validators = Set::new(next_validator_response.validators, None);

    Ok(LightBlock::new(
        signed_header,
        validators,
        next_validators,
        // TODO do we care about this ID?
        Id::new([0; 20]),
    ))
}

/// Fetch all light client blocks necessary to prove a given range.
pub async fn fetch_light_blocks(
    client: Arc<HttpClient>,
    range: Range<u64>,
) -> anyhow::Result<Vec<LightBlock>> {
    tracing::debug!("Fetching light blocks");
    let mut all_blocks = Vec::with_capacity(range.end.saturating_sub(range.start) as usize);

    // Define maximum number of parallel requests.
    let semaphore = Arc::new(Semaphore::new(10));
    let mut jhs = Vec::new();
    for height in range {
        let semaphore = semaphore.clone();
        let client = client.clone();
        let jh: JoinHandle<anyhow::Result<_>> = tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;
            let response = fetch_light_block(&client, Height::try_from(height)?).await?;
            drop(_permit);

            Ok(response)
        });
        jhs.push(jh);
    }
    // Collect responses from tasks.
    for jh in jhs {
        let response = jh.await??;
        all_blocks.push(response);
    }

    Ok(all_blocks)
}

/// Prove a single block with the trusted light client block and the height to fetch and prove.
#[instrument(skip(input), fields(light_range = ?input.untrusted_height()..input.trusted_height()), err, level = Level::DEBUG)]
pub async fn prove_block(input: LightBlockProveData) -> anyhow::Result<Receipt> {
    // TODO remove the need to serialize with cbor
    // TODO a self-describing serialization protocol needs to be used with serde because the
    //      LightBlock type requires it. Seems like proto would be most stable format, rather than
    //      one used for RPC.
    let mut input_serialized = Vec::new();
    ciborium::into_writer(&input, &mut input_serialized)?;

    tracing::debug!("Proving light client");
    // Note: must be in blocking context to not have issues with Bonsai blocking client when selected
    let prove_info = tokio::task::spawn_blocking(move || {
        let env = ExecutorEnv::builder()
            .write_slice(&input_serialized)
            .build()?;

        let prover = default_prover();
        prover.prove(env, TM_LIGHT_CLIENT_ELF)
    })
    .await??;
    let receipt = prove_info.receipt;

    let commit: LightClientCommit = receipt.journal.decode()?;
    // Assert that the data root equals what is committed from the proof.
    assert_eq!(
        input
            .untrusted_block
            .signed_header
            .header()
            .hash()
            .as_bytes(),
        &commit.next_block_hash
    );

    Ok(receipt)
}

/// Fetches and proves a range of light client blocks.
#[instrument(skip(client), err, level = Level::INFO)]
pub async fn prove_block_range(
    client: Arc<HttpClient>,
    range: Range<u64>,
) -> anyhow::Result<Receipt> {
    let prover = default_prover();

    // Include fetching the trusted light client block from before the range.
    // TODO possibly worth chunking this to avoid
    let light_blocks = fetch_light_blocks(client.clone(), range.start - 1..range.end).await?;
    let (trusted_block, blocks) = light_blocks
        .split_first()
        .context("range cannot be empty")?;
    let range_iterator = LightBlockRangeIterator {
        trusted_block,
        blocks,
    };

    let mut batch_env_builder = ExecutorEnv::builder();
    let mut batch_receipts = Vec::new();
    for inputs in range_iterator {
        // TODO this will likely have to check chain height and wait for new block to be published
        //      or have a separate function do this.
        let receipt = prove_block(inputs).await?;

        batch_receipts.push(ByteBuf::from(receipt.journal.bytes.clone()));
        batch_env_builder.add_assumption(receipt);
    }

    let env = batch_env_builder.write(&batch_receipts)?.build()?;

    // Note: must block in place to not have issues with Bonsai blocking client when selected
    tracing::debug!("Proving batch of blocks");
    // TODO likely better to move this to a spawn blocking, prover and env types not compat, messy
    let prove_info = tokio::task::block_in_place(move || {
        prover.prove_with_opts(env, BATCH_GUEST_ELF, &ProverOpts::groth16())
    })?;

    Ok(prove_info.receipt)
}

/// Post batch proof to Eth based chain.
#[instrument(skip(contract, receipt), err, level = Level::DEBUG)]
pub async fn post_batch<T, P, N>(
    contract: &IBlobstreamInstance<T, P, N>,
    receipt: &Receipt,
) -> anyhow::Result<TxHash>
where
    T: Transport + Clone,
    P: Provider<T, N>,
    N: Network,
{
    tracing::debug!("Posting batch (dev mode={})", is_dev_mode());
    let seal = match is_dev_mode() {
        true => [&[0u8; 4], receipt.claim()?.digest().as_bytes()].concat(),
        false => groth16::encode(receipt.inner.groth16()?.seal.clone())?,
    };

    let range_commitment = RangeCommitment::abi_decode(&receipt.journal.bytes, true)?;

    let res = contract
        .updateRange(range_commitment, seal.into())
        .send()
        .await?
        .watch()
        .await?;

    Ok(res)
}
