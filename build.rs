use std::{
    env::{var, var_os},
    process::Command,
};

fn main() {
    // Re-run the build script if any CoreMark/porting sources change.
    println!("cargo:rerun-if-changed=coremark/core_list_join.c");
    println!("cargo:rerun-if-changed=coremark/core_main.c");
    println!("cargo:rerun-if-changed=coremark/core_matrix.c");
    println!("cargo:rerun-if-changed=coremark/core_state.c");
    println!("cargo:rerun-if-changed=coremark/core_util.c");
    println!("cargo:rerun-if-changed=coremark/coremark.h");
    println!("cargo:rerun-if-changed=portme/core_portme.c");
    println!("cargo:rerun-if-changed=portme/core_portme.h");

    let target = var("TARGET").unwrap_or_default();

    let mut build = cc::Build::new();
    // Build CoreMark as a C static library. We rename C `main` so it can be linked into
    // this Rust crate without conflicting with Rust's `main`.
    build
        .include("coremark")
        .include("portme")
        .file("coremark/core_list_join.c")
        .file("coremark/core_main.c")
        .file("coremark/core_matrix.c")
        .file("coremark/core_state.c")
        .file("coremark/core_util.c")
        .file("portme/core_portme.c")
        .define("main", Some("coremark_main"))
        .define("FLAGS_STR", Some("\"(set by build.rs)\""))
        .flag_if_supported("-std=c11");

    // When building the OpenVM guest target, cc-rs will add `-march=rv32im -mabi=ilp32`, but
    // the default host compiler may treat those as x86 flags unless we set a RISC-V target.
    if target == "riscv32im-risc0-zkvm-elf" {
        // Respect user/toolchain-provided compiler overrides. For example, one could:
        //   export CC_riscv32im_risc0_zkvm_elf=riscv64-unknown-elf-gcc
        //   export AR_riscv32im_risc0_zkvm_elf=riscv64-unknown-elf-ar
        let cc_target_underscored = format!("CC_{}", target.replace('-', "_"));
        let user_specified_cc = var_os("CC").is_some()
            || var_os(&cc_target_underscored).is_some()
            || var_os(format!("CC_{}", target)).is_some();

        if !user_specified_cc {
            // Use GCC for the RISC-V guest build (no clang fallback). Many setups may ship
            // `riscv64-unknown-elf-gcc` (which can still target rv32 via -march/-mabi), so
            // we try a small set of common RISC-V GCC names.
            let mut candidates = Vec::new();
            if let Ok(v) = var("OPENVM_GUEST_GCC") {
                candidates.push(v);
            }

            candidates.extend(
                [
                    "riscv32-unknown-elf-gcc",
                    "riscv64-unknown-elf-gcc",
                    "riscv32-linux-gnu-gcc",
                    "riscv64-linux-gnu-gcc",
                    "riscv-none-elf-gcc",
                    "riscv64-unknown-linux-gnu-gcc",
                ]
                .into_iter()
                .map(|s| s.to_string()),
            );

            let gcc = candidates
                .into_iter()
                .find(|gcc| Command::new(gcc).arg("--version").output().is_ok())
                .unwrap_or_else(|| panic!("Guest build requires a RISC-V GCC in PATH."));
            build.compiler(gcc);
        }
    }

    build.compile("coremark");
}
