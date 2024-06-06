use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv};

const ELF: &[u8] = include_bytes!("../../target/riscv32im-risc0-zkvm-elf/release/guest");

fn main() -> anyhow::Result<()> {
    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let guest_id = compute_image_id(ELF).unwrap();

    let input: u32 = 15 * u32::pow(2, 27) + 1;
    let env = ExecutorEnv::builder().write(&input)?.build()?;

    let prover = default_prover();

    let prove_info = prover.prove(env, ELF)?;
    let receipt = prove_info.receipt;

    receipt.verify(guest_id)?;

    Ok(())
}
