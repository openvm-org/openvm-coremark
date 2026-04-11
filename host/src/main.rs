#![cfg_attr(feature = "tco", allow(incomplete_features))]
#![cfg_attr(feature = "tco", feature(explicit_tail_calls))]
use std::time::Instant;

use clap::Parser;
use openvm_circuit::arch::instructions::exe::VmExe;
use openvm_sdk::{
    config::{AggregationSystemParams, AppConfig},
    Sdk,
};
use openvm_sdk_config::{SdkVmConfig, TranspilerConfig};
use openvm_stark_sdk::{
    bench::run_with_metric_collection, config::app_params_with_100_bits_security,
    openvm_stark_backend::codec::Encode,
};
use openvm_transpiler::{elf::Elf, openvm_platform::memory::MEM_SIZE, FromElf};
use openvm_verify_stark_host::{verify_vm_stark_proof_decoded, vk::VmStarkVerifyingKey};
use tracing::info;

const COREMARK_ELF: &[u8] = include_bytes!("../elf/openvm-coremark");

const DEFAULT_LOG_STACKED_HEIGHT: usize = 24;
const VM_MAX_CONSTRAINT_DEGREE: usize = 4;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BenchMode {
    Execute,
    ExecuteMetered,
    ProveApp,
    ProveStark,
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(long, value_enum, default_value = "prove-stark")]
    mode: BenchMode,

    #[arg(long, alias = "max_segment_length")]
    max_segment_length: Option<u32>,

    #[arg(long)]
    segment_max_memory: Option<usize>,
}

fn main() -> eyre::Result<()> {
    let args = Args::parse();

    #[cfg(feature = "cuda")]
    println!("CUDA Backend Enabled");

    let mut vm_config = SdkVmConfig::standard();
    vm_config.system.config = vm_config
        .system
        .config
        .with_max_constraint_degree(VM_MAX_CONSTRAINT_DEGREE)
        .with_public_values(32);

    if let Some(max_trace_height) = args.max_segment_length {
        vm_config
            .as_mut()
            .segmentation_config
            .limits
            .set_max_trace_height(max_trace_height);
    }
    if let Some(max_memory) = args.segment_max_memory {
        vm_config
            .as_mut()
            .segmentation_config
            .limits
            .set_max_memory(max_memory);
    }

    let transpiler = vm_config.transpiler().clone();
    let app_params = app_params_with_100_bits_security(DEFAULT_LOG_STACKED_HEIGHT);
    let app_config = AppConfig::new(vm_config, app_params);
    let agg_params = AggregationSystemParams::default();
    let sdk = Sdk::new(app_config, agg_params)?;

    let elf = Elf::decode(COREMARK_ELF, MEM_SIZE as u32)?;
    let exe = VmExe::from_elf(elf, transpiler)?;

    // Coremark takes no input
    let stdin = vec![].into();

    run_with_metric_collection("OUTPUT_PATH", move || -> eyre::Result<()> {
        let start = Instant::now();
        match args.mode {
            BenchMode::Execute => {
                let public_values = sdk.execute(exe, stdin)?;
                info!(
                    "Execute completed, public values len: {}",
                    public_values.len()
                );
            }
            BenchMode::ExecuteMetered => {
                let (public_values, segments) = sdk.execute_metered(exe, stdin)?;
                info!(
                    "Execute metered completed, public values len: {}",
                    public_values.len()
                );
                println!("BENCH_NUM_SEGMENTS={}", segments.len());
            }
            BenchMode::ProveApp => {
                let mut prover = sdk.app_prover(exe)?;
                prover.set_program_name("coremark");
                let app_proof = prover.prove(stdin)?;
                println!("BENCH_NUM_SEGMENTS={}", app_proof.per_segment.len());
            }
            BenchMode::ProveStark => {
                let (proof, baseline) = sdk.prove(exe, stdin, &[])?;
                let vk = VmStarkVerifyingKey {
                    mvk: (*sdk.agg_vk()).clone(),
                    baseline,
                };
                let encoded = proof.encode_to_vec()?;
                let compressed = zstd::encode_all(&encoded[..], 19)?;
                info!(
                    "Proof Size (bytes): {}, Compressed Size: {}",
                    encoded.len(),
                    compressed.len()
                );
                verify_vm_stark_proof_decoded(&vk, &proof)?;
                info!("Proof verified successfully!");
            }
        }
        let elapsed = start.elapsed();
        info!("Total time: {:.3}s", elapsed.as_secs_f64());
        Ok(())
    })?;

    Ok(())
}
