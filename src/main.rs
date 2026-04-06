use eyre::Result;
use marlin::verilator::VerilatedModelConfig;
use tta_sim::{create_simtop_runtime, SimTop, SimTopHarness};

struct CliOptions {
    cycles: u32,
    reset_cycles: u32,
    sram_words: usize,
    trace_file: Option<String>,
}

fn parse_args() -> Result<CliOptions> {
    let mut cycles = 200;
    let mut reset_cycles = 100;
    let mut sram_words = 1 << 19;
    let mut trace_file = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cycles" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--cycles requires a value"))?;
                cycles = value.parse()?;
            }
            "--reset-cycles" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--reset-cycles requires a value"))?;
                reset_cycles = value.parse()?;
            }
            "--sram-words" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--sram-words requires a value"))?;
                sram_words = value.parse()?;
            }
            "--trace-file" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--trace-file requires a path"))?;
                trace_file = Some(value);
            }
            "--help" | "-h" => {
                println!("Usage: tta-sim [--cycles N] [--reset-cycles N] [--sram-words N] [--trace-file PATH]");
                std::process::exit(0);
            }
            other => {
                return Err(eyre::eyre!("Unknown argument: {}", other));
            }
        }
    }

    Ok(CliOptions {
        cycles,
        reset_cycles,
        sram_words,
        trace_file,
    })
}

fn main() -> Result<()> {
    env_logger::init();
    println!("🚀 TTA Rust Simulator Starting...");
    let options = parse_args()?;

    let runtime = create_simtop_runtime()?;
    let simtop = runtime
        .create_model::<SimTop>(&VerilatedModelConfig {
            enable_tracing: options.trace_file.is_some(),
            ..Default::default()
        })
        .map_err(|e| eyre::eyre!("Failed to create simtop model: {:?}", e))?;
    let mut harness = SimTopHarness::new(simtop, options.sram_words);
    let mut trace = options
        .trace_file
        .as_ref()
        .map(|path| harness.model_mut().open_vcd(path.as_str()));

    println!("✅ simtop model created");
    harness.reset(options.reset_cycles);
    println!("🔄 Reset complete after {} cycles", options.reset_cycles);

    for _ in 0..options.cycles {
        harness.step_cycle();
        if let Some(vcd) = trace.as_mut() {
            vcd.dump(harness.ticks());
        }
    }

    println!("✅ Completed {} simulation cycles", options.cycles);
    println!("🎉 Simulation finished");
    Ok(())
}
