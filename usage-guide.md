## Running service

### Local testing

Start up a local Eth dev node:
```console
anvil
```

Deploy verifier and blobstream contract

```console
RUST_LOG=info cargo run -- deploy --eth-rpc http://127.0.0.1:8545 --private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --tm-height 10 --tm-block-hash 5D3BDD6B58620A0B6C5A9122863D11DA68EB18935D12A9F4E4CF1A27EB39F1AC --dev
```

> The `--tm-height` and `--tm-block-hash` options are pulled from the network that is being synced. Make sure these match the network from `--tendermint-rpc` in the following command.

Start the service:

```
RISC0_DEV_MODE=true RUST_LOG=host=trace,info cargo run -p service -- --tendermint-rpc https://rpc.celestia-mocha.com --eth-rpc http://127.0.0.1:8545/ --eth-address 0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512 --private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --batch-size 4
```

Where the `--tendermint-rpc` param can be configured to be any other network endpoint, and the `--batch-size` can be configured.

> Note: The `--eth-address` here is hard coded to be the first deployment when running the first deployment. Either restart the anvil node or update the `--eth-address` parameter to the output from the deploy if making changes to the contract.


#### Local with snark proofs from Bonsai

The flow is similar to above, except that the `--dev` flag is removed from the deployment, to deploy the groth16 verifier in its place for that step:

```
RUST_LOG=info cargo run -- deploy --eth-rpc http://127.0.0.1:8545 --private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --tm-height 10 --tm-block-hash 5D3BDD6B58620A0B6C5A9122863D11DA68EB18935D12A9F4E4CF1A27EB39F1AC
```

### Sepolia

Currently there are already groth16 and mock verifiers deployed to Sepolia.

- Sepolia groth16 verifier: `0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187`
- Sepolia mock verifier: `0x6e5D3e69934814470CEE91C63afb7033056682F3`

Deploy the blobstream contract with the `--verifier-address` from above:

```
RUST_LOG=info cargo run -- deploy --eth-rpc https://ethereum-sepolia-rpc.publicnode.com --private-key-hex <ADD KEY HERE> --tm-height 1802142 --tm-block-hash 6D8FD8ADC8FBD5E7765EC557D9DF86041F63F9109202A888D8D246B3BCC3B46A --verifier-address 0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187
```

Run the service with `RISC0_DEV_MODE=true` if you chose the mock verifier.

```
RUST_LOG=host=trace,info cargo run -p service --release -- --tendermint-rpc https://rpc.celestia-mocha.com --eth-rpc https://ethereum-sepolia-rpc.publicnode.com --eth-address <BLOBSTREAM ADDRESS FROM DEPLOY> --private-key-hex <ADD KEY HERE> --batch-size 16
```