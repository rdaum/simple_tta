use eyre::Result;
use tta_sim::{create_tta_runtime, test_basic_reset_sequence, TtaTestbench};

fn main() -> Result<()> {
    env_logger::init();
    println!("🚀 TTA Rust Simulator Starting...");

    let runtime = create_tta_runtime()?;
    let mut tta = runtime
        .create_model_simple::<TtaTestbench>()
        .map_err(|e| eyre::eyre!("Failed to create TTA model: {:?}", e))?;

    println!("✅ TTA processor model created!");

    // Run basic functionality test
    test_basic_reset_sequence(&mut tta)?;

    println!("🎉 All tests passed!");
    Ok(())
}
