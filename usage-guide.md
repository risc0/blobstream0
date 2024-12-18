## Running the Blobstream Zero Service

This service will watch the Ethereum contract for the current head of the verified chain, request light client blocks from a Tendermint node, generate a batch proof, and then post that proof back to the Ethereum contract.

### Local testing

Start up a local Eth dev node:
```console
anvil
```

Deploy verifier and blobstream contract

```console
RUST_LOG=info cargo run -p blobstream0 -- deploy \
	--eth-rpc http://127.0.0.1:8545 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--tm-height 9 \
	--tm-block-hash 5C5451567973D8658A607D58F035BA9078291E33D880A0E6E67145C717E6B11B \
	--min-batch-size 7 \
	--dev
```

> The `--tm-height` and `--tm-block-hash` options are pulled from the network that is being synced. Make sure these match the network from `--tendermint-rpc` in the following command.

Start the service:

```
RISC0_DEV_MODE=true RUST_LOG=blobstream0=debug,info cargo run -p blobstream0 -- service \
	--tendermint-rpc https://celestia-testnet.brightlystake.com \
	--eth-rpc http://127.0.0.1:8545/ \
	--eth-address 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--batch-size 64
```

Where the `--tendermint-rpc` param can be configured to be any other network endpoint, and the `--batch-size` can be configured.

> Note: The `--eth-address` here is hard coded to be the printed address when running the first deployment. Either restart the anvil node or update the `--eth-address` parameter to the output from the deploy if making changes to the contract.


#### Local with snark proofs from Bonsai

The flow is similar to above, except that the `--dev` flag is removed from the deployment, to deploy the groth16 verifier in its place for that step:

Set the Bonsai env variables with the url and API key:

```console
export BONSAI_API_KEY=<YOUR_API_KEY>
export BONSAI_API_URL=<BONSAI_URL>
```

> Note: you can instead use local proving and not set these env variables if on an x86 machine. See more https://dev.risczero.com/api/next/generating-proofs/proving-options

```
RUST_LOG=info,blobstream0=debug cargo run -p blobstream0 -- deploy \
	--eth-rpc http://127.0.0.1:8545 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--tm-height 9 \
	--tm-block-hash 5C5451567973D8658A607D58F035BA9078291E33D880A0E6E67145C717E6B11B \
	--min-batch-size 7
```

### Sepolia

Currently there are already groth16 and mock verifiers deployed to Sepolia.

- Sepolia groth16 verifier: `0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187`
- Sepolia mock verifier: `0x6e5D3e69934814470CEE91C63afb7033056682F3`

Deploy the blobstream contract with the `--verifier-address` from above:

```
RUST_LOG=info,blobstream0=debug cargo run -p blobstream0 -- deploy \
	--eth-rpc https://ethereum-sepolia-rpc.publicnode.com \
	--private-key-hex <ADD KEY HERE> \
	--tm-height 1802142 \
	--tm-block-hash 6D8FD8ADC8FBD5E7765EC557D9DF86041F63F9109202A888D8D246B3BCC3B46A \
	--verifier-address 0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187 \
	--min-batch-size 7
```

Run the service with `RISC0_DEV_MODE=true` if you chose the mock verifier.

```
RUST_LOG=blobstream0=debug,info cargo run -p blobstream0 --release -- service \
	--tendermint-rpc https://celestia-testnet.brightlystake.com \
	--eth-rpc https://ethereum-sepolia-rpc.publicnode.com \
	--eth-address <BLOBSTREAM ADDRESS FROM DEPLOY> \
	--private-key-hex <ADD KEY HERE> \
	--batch-size 16
```

### Dockerized service

```console
# Build the guest ELF in docker
RISC0_USE_DOCKER=true cargo c

# Build the docker image, which will use the docker built guest ELF
docker build -f dockerfiles/blobstream0.Dockerfile --platform linux/amd64 -t blobstream0 .
```

And can use the `blobstream0` binary as:

```console
docker run blobstream0 deploy \
	--eth-rpc http://host.docker.internal:8545 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--tm-height 9 \
	--tm-block-hash 5C5451567973D8658A607D58F035BA9078291E33D880A0E6E67145C717E6B11B \
	--min-batch-size 7
```

```console
docker run -e RUST_LOG=blobstream0=debug,info -e BONSAI_API_KEY=<API KEY> -e BONSAI_API_URL=https://api.bonsai.xyz blobstream0 service \
	--tendermint-rpc https://celestia-testnet.brightlystake.com \
	--eth-rpc http://host.docker.internal:8545/ \
	--eth-address 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--batch-size 64


### Admin transaction examples:


```
cast send --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "adminSetImageId(bytes32)" <IMAGE ID>

cast send --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "adminSetVerifier(address)" 0x5FbDB2315678afecb367f032d93F642f64180aa3

cast send --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "adminSetTrustedState(bytes32,uint64)" 0x5C5451567973D8658A607D58F035BA9078291E33D880A0E6E67145C717E6B11B 9
```

#### Upgrading

CLI:

```
RUST_LOG=info,blobstream0=debug cargo run -p blobstream0 -- upgrade \
	--eth-rpc http://127.0.0.1:8545 \
	--private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
	--proxy-address 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0
```

Cast:

```
cast send --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "upgradeToAndCall(address,bytes)" 0x9A676e781A523b5d0C0e43731313A708CB607508 0x
```

Get the current implementation address:

```
cast storage 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
```

#### Ownership transfer

Get current owner:

```
cast call 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "owner()(address)"
```

```
cast send --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "transferOwnership(address)" 0x70997970C51812dc3A010C7d01b50e0d17dc79C8

cast send --private-key 0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0 "acceptOwnership()"


```

> Addresses and private key from default anvil test node