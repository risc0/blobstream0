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

use clap::Parser;
use host::prove_block_range;
use tendermint_rpc::HttpClient;
use tokio::fs;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let Args {
        start,
        end,
        tendermint_rpc,
        out,
    } = Args::parse();

    let client = HttpClient::new(tendermint_rpc.as_str())?;

    let receipt = prove_block_range(&client, start..end).await?;

    fs::write(out, bincode::serialize(&receipt)?).await?;

    Ok(())
}
