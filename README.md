# Blobstream Zero

> WARNING: This project is currently experimental and not recommended for any production use cases yet

Blobstream Zero is an implementation of [Blobstream](https://docs.celestia.org/developers/blobstream) using the [RISC Zero zkVM](https://www.risczero.com/) to verify Celestia blocks.

The blocks are verified using a zk Tendermint light client with the [light-client-guest](./light-client-guest/guest/src/main.rs) program, which is composed with a proof that recursively verifies these light client proofs and builds a merkle tree of the verified blocks in [batch-guest](./batch-guest/guest/src/main.rs). This proof is then verified on Ethereum using the [Blobstream solidity contracts](./contracts/src/).

The ABI for the Blobstream Zero solidity contract as well as the merkle tree format for the batch proof is currently matching previous implementations to maintain as much compatibility with previous solutions as well as the [existing APIs to request Blobstream inclusion proofs](https://docs.celestia.org/developers/blobstream-proof-queries#_1-data-root-inclusion-proof).

### Usage

Clone this repository, then pull submodules:

```console
git submodule update --init --recursive
```

Ensure [Rust](https://www.rust-lang.org/tools/install), [Foundry](https://book.getfoundry.sh/getting-started/installation), and [the RISC Zero toolchain](https://dev.risczero.com/api/zkvm/install) are installed.

Build guest programs and update autogenerated files:

```console
# Could also run `cargo build`
cargo check
```

Optionally run tests, which includes an [end to end test](./host/tests/e2e_test.rs):

```console
cargo test
```

Run the CLI to generate proofs or post those proofs on an Eth based network:

```console
cargo run -p blobstream0-cli -- --help
```

For docs on running the Blobstream service, see [usage-guide.md](./usage-guide.md).

> Note: This CLI as well as other APIs will change in the short term. If you need anything specific from this, [open an issue](https://github.com/risc0/blobstream0/issues/new)!