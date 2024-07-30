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
use batch_guest::BATCH_GUEST_ELF;
use blobstream0_primitives::{
    proto::{TrustedLightBlock, UntrustedLightBlock},
    IBlobstream::{IBlobstreamInstance, RangeCommitment},
    LightBlockProveData, LightClientCommit,
};
use light_client_guest::TM_LIGHT_CLIENT_ELF;
use risc0_ethereum_contracts::groth16;
use risc0_zkvm::{default_prover, is_dev_mode, sha::Digestible, ExecutorEnv, ProverOpts, Receipt};
use serde_bytes::ByteBuf;
use std::{ops::Range, sync::Arc};
use tendermint::{block::Height, validator::Set};
use tendermint_light_client_verifier::types::Header;
use tendermint_proto::{types::Header as ProtoHeader, Protobuf};
use tendermint_rpc::{Client, HttpClient, Paging};
use tokio::{sync::Semaphore, task::JoinHandle};
use tracing::{instrument, Level};

mod range_iterator;
use range_iterator::LightBlockRangeIterator;

/// Currently set to the max allowed by tendermint RPC
const HEADER_REQ_COUNT: u64 = 20;

async fn fetch_validators(client: &HttpClient, block_height: Height) -> anyhow::Result<Set> {
    // Note: This currently needs to use Paging::All or the hash mismatches.
    let validator_response = client.validators(block_height, Paging::All).await?;

    let validators = Set::new(validator_response.validators, None);

    Ok(validators)
}

async fn fetch_trusted_light_block(
    client: &HttpClient,
    block_height: Height,
) -> anyhow::Result<TrustedLightBlock> {
    let commit_response = client.commit(block_height).await?;
    let signed_header = commit_response.signed_header;

    let next_validators = fetch_validators(client, block_height.increment()).await?;

    Ok(TrustedLightBlock {
        signed_header,
        next_validators,
    })
}

async fn fetch_untrusted_light_block(
    client: &HttpClient,
    block_height: Height,
) -> anyhow::Result<UntrustedLightBlock> {
    let commit_response = client.commit(block_height).await?;
    let signed_header = commit_response.signed_header;

    let validators = fetch_validators(client, block_height).await?;

    Ok(UntrustedLightBlock {
        signed_header,
        validators,
    })
}

/// Fetch all light client blocks necessary to prove a given range.
pub async fn fetch_headers(
    client: Arc<HttpClient>,
    range: Range<u64>,
) -> anyhow::Result<Vec<Header>> {
    tracing::debug!(target: "blobstream0::core", "Fetching light blocks");
    let mut all_blocks = Vec::with_capacity(range.end.saturating_sub(range.start) as usize);

    let mut curr = range.start;
    // Define maximum number of parallel requests.
    let semaphore = Arc::new(Semaphore::new(16));
    let mut jhs = Vec::new();
    while curr < range.end {
        let semaphore = semaphore.clone();
        let client = client.clone();
        let start_height = curr;
        curr += HEADER_REQ_COUNT;
        // Note: range end is inclusive for Tendermint, so end max is decremented.
        let end_height = std::cmp::min(curr, range.end) - 1;
        let jh: JoinHandle<anyhow::Result<_>> = tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;
            tracing::debug!(
                target: "blobstream0::core",
                "requesting header range {}-{}",
                start_height, end_height
            );
            let response = client
                .blockchain(
                    Height::try_from(start_height)?,
                    Height::try_from(end_height)?,
                )
                .await?;
            drop(_permit);

            // Headers are returned in reverse order, reorder
            let headers: Vec<Header> = response
                .block_metas
                .into_iter()
                .rev()
                .map(|b| b.header)
                .collect();
            Ok(headers)
        });
        jhs.push(jh);
    }
    // Collect responses from tasks.
    for jh in jhs {
        let response = jh.await??;
        all_blocks.extend(response);
    }

    Ok(all_blocks)
}

/// Prove a single block with the trusted light client block and the height to fetch and prove.
#[instrument(
    target = "blobstream0::core",
    skip(input),
    fields(light_range = ?input.untrusted_height()..input.trusted_height()),
    err, level = Level::DEBUG)]
pub async fn prove_block(input: LightBlockProveData) -> anyhow::Result<Receipt> {
    let mut buffer = Vec::<u8>::new();
    assert_eq!(
        input.untrusted_height() - input.trusted_height() - 1,
        input.interval_headers.len() as u64
    );
    let expected_next_hash = input.untrusted_block.signed_header.header().hash();

    TrustedLightBlock {
        signed_header: input.trusted_block.signed_header,
        next_validators: input.trusted_block.next_validators,
    }
    .encode_length_delimited(&mut buffer)?;

    UntrustedLightBlock {
        signed_header: input.untrusted_block.signed_header,
        validators: input.untrusted_block.validators,
    }
    .encode_length_delimited(&mut buffer)?;

    for header in input.interval_headers {
        Protobuf::<ProtoHeader>::encode_length_delimited(header, &mut buffer)?;
    }

    let buffer_len: u32 = buffer
        .len()
        .try_into()
        .expect("buffer cannot exceed 32 bit range");

    tracing::debug!(target: "blobstream0::core", "Proving light client");
    // Note: must be in blocking context to not have issues with Bonsai blocking client when selected
    let prove_info = tokio::task::spawn_blocking(move || {
        let env = ExecutorEnv::builder()
            .write(&buffer_len)?
            .write_slice(&buffer)
            .build()?;

        let prover = default_prover();
        prover.prove(env, TM_LIGHT_CLIENT_ELF)
    })
    .await??;
    let receipt = prove_info.receipt;

    let commit: LightClientCommit = receipt.journal.decode()?;
    // Assert that the data root equals what is committed from the proof.
    assert_eq!(expected_next_hash.as_bytes(), &commit.next_block_hash);

    Ok(receipt)
}

/// Fetches and proves a range of light client blocks.
#[instrument(target = "blobstream0::core", skip(client), err, level = Level::INFO)]
pub async fn prove_block_range(
    client: Arc<HttpClient>,
    range: Range<u64>,
) -> anyhow::Result<Receipt> {
    let prover = default_prover();

    // Include fetching the trusted light client block from before the range.
    let (trusted_block, blocks) = tokio::try_join!(
        fetch_trusted_light_block(&client, Height::try_from(range.start - 1)?),
        fetch_headers(client.clone(), range.start..range.end)
    )?;

    let mut range_iterator = LightBlockRangeIterator {
        client: &client,
        trusted_block,
        blocks: &blocks,
    };

    let mut batch_env_builder = ExecutorEnv::builder();
    let mut batch_receipts = Vec::new();
    while let Some(inputs) = range_iterator.next_range().await? {
        // TODO this will likely have to check chain height and wait for new block to be published
        //      or have a separate function do this.
        let receipt = prove_block(inputs).await?;

        batch_receipts.push(ByteBuf::from(receipt.journal.bytes.clone()));
        batch_env_builder.add_assumption(receipt);
    }

    let env = batch_env_builder.write(&batch_receipts)?.build()?;

    // Note: must block in place to not have issues with Bonsai blocking client when selected
    tracing::debug!(target: "blobstream0::core", "Proving batch of blocks");
    // TODO likely better to move this to a spawn blocking, prover and env types not compat, messy
    let prove_info = tokio::task::block_in_place(move || {
        prover.prove_with_opts(env, BATCH_GUEST_ELF, &ProverOpts::groth16())
    })?;

    Ok(prove_info.receipt)
}

/// Post batch proof to Eth based chain.
#[instrument(target = "blobstream0::core", skip(contract, receipt), err, level = Level::DEBUG)]
pub async fn post_batch<T, P, N>(
    contract: &IBlobstreamInstance<T, P, N>,
    receipt: &Receipt,
) -> anyhow::Result<TxHash>
where
    T: Transport + Clone,
    P: Provider<T, N>,
    N: Network,
{
    tracing::debug!(target: "blobstream0::core", "Posting batch (dev mode={})", is_dev_mode());
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
