An escrow program (smart contract) for the Solana blockchain.

The program holds funds offered by the initializer (party A) in a temp token account until the taker (party B)
accepts the swap. Once party B agrees and signs the transaction, the swap is executed and both temporary token accounts
as well as the PDA escrow account containing transaction state are deleted.

### Environment Setup
1. Install Rust from https://rustup.rs/
2. Install Solana v1.6.2 or later from https://docs.solana.com/cli/install-solana-cli-tools#use-solanas-install-tool

### Build and test for program compiled natively
```
$ cargo build
$ cargo test
```

### Build and test the program compiled for BPF
```
$ cargo build-bpf
$ cargo test-bpf
```
