```
anvil
```

```
cargo run -- deploy --eth-rpc http://127.0.0.1:8545 --private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --tm-height 10 --tm-block-hash 5D3BDD6B58620A0B6C5A9122863D11DA68EB18935D12A9F4E4CF1A27EB39F1AC --dev
```

```
RISC0_DEV_MODE=true cargo run -p service -- --tendermint-rpc https://rpc.celestia-mocha.com --eth-rpc http://127.0.0.1:8545/ --eth-address 0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512 --private-key-hex 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --batch-size 5
```
