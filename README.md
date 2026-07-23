# openvm-coremark

The [CoreMark](coremark/README.md) benchmark as an OpenVM guest program.
The repo root is the guest crate, and `host/` contains a separate host-side harness.
The guest program can be executed and/or proven using either `cargo openvm` or the host-side harness.

## Getting Started

### Prerequisites

#### Required:

- `git`
- `rustup` with the repo toolchain from `rust-toolchain.toml`
- `cargo openvm` (install via the [official OpenVM docs](https://docs.openvm.dev/book/getting-started/introduction))
- The matching OpenVM guest toolchain (installed with `cargo openvm toolchain install`)
- A RISC-V GCC toolchain in `PATH` for guest builds

This repository tracks OpenVM's `develop-v2.1.0` branch, so use a compatible
version of `cargo openvm`.

#### Optional:

- NVIDIA tooling for CUDA/profiling flows: `nvidia-smi`, `compute-sanitizer`, and `nsys`
- LLVM Clang 19 or newer (Clang 22 recommended) and `lld` for accelerated RVR execution
  in the x86_64 host harness; set `RVR_CC` if the compiler is not available as `clang-22`

If you want to use a specific RISC-V GCC for guest builds, set `OPENVM_GUEST_GCC`.
Otherwise `build.rs` tries common toolchain names in `PATH`:
`riscv64-unknown-elf-gcc`, `riscv64-linux-gnu-gcc`, `riscv-none-elf-gcc`,
and `riscv64-unknown-linux-gnu-gcc`. The matching archiver is selected
automatically; it can be overridden with `OPENVM_GUEST_AR`.

#### Installing a RISC-V GCC toolchain

On Ubuntu or Debian, the same package used by CI is:

```bash
sudo apt-get update
sudo apt-get install -y gcc-riscv64-unknown-elf
```

Then verify that one of the supported compiler names is available:

```bash
command -v riscv64-unknown-elf-gcc
riscv64-unknown-elf-gcc --version
```

If your system installs a different supported binary name, that is also fine as
long as it is on `PATH`. If you want to force a specific compiler, export it
explicitly before building:

```bash
export OPENVM_GUEST_GCC=riscv64-unknown-elf-gcc
```

Install the matching RV64 guest toolchain:

```bash
cargo openvm toolchain install
```

### Clone The Repo

```bash
git clone --recurse-submodules <repo-url>
cd openvm-coremark
```

If you already cloned without submodules:

```bash
git submodule update --init --recursive
```

### Running/Proving Using `cargo openvm`

To build and run the CoreMark guest program using OpenVM:

```bash
cargo openvm run
```

To generate an app proof of the CoreMark execution:

```bash
cargo openvm keygen --app-only
cargo openvm prove app
```

To generate an aggregated STARK proof of the CoreMark execution:

```bash
cargo openvm setup
cargo openvm keygen
cargo openvm prove stark
```

To use a fixed iteration count instead of the default build-time setting (i.e. 10000):

```bash
CFLAGS="-DITERATIONS=1000" cargo openvm run
CFLAGS="-DITERATIONS=1000" cargo openvm prove app
CFLAGS="-DITERATIONS=1000" cargo openvm prove stark
```

For more information on `cargo openvm` usage, see the [official OpenVM docs](https://docs.openvm.dev/book/getting-started/introduction).

### Running/Proving Using the Host Harness

The host harness currently expects a guest ELF at `host/elf/openvm-coremark`.
Build the guest ELF with `cargo openvm build`, and then copy the resulting ELF there:

```bash
mkdir -p host/elf
cp target/riscv64im-unknown-openvm-elf/<profile>/openvm-coremark host/elf/openvm-coremark
```

Then run the host wrapper:

```bash
./host/scripts/run_coremark.sh
```

### Benchmarking

For the `cargo openvm` flow, measure elapsed execution/proving time outside the guest
with a shell timing utility such as:

```bash
time cargo openvm run
```

The in-guest CoreMark timing hooks are stubbed, so the benchmark's printed
timing-derived fields are not meaningful in this repo's current setup.

For the host-harness flow, `./host/scripts/run_coremark.sh` writes `metrics.json`
in the normal non-`--nsys` path. You can use [`openvm-prof`](https://github.com/openvm-org/openvm/tree/main/crates/prof)
on that `metrics.json` output for profiling and benchmark analysis.

## Repo structure

```text
openvm-coremark/
├── Cargo.toml                 # Guest crate manifest
├── build.rs                   # Builds CoreMark C sources into the guest crate
├── openvm.toml                # Guest VM configuration
├── src/
│   └── main.rs                # OpenVM guest entrypoint
├── portme/
│   ├── core_portme.c          # CoreMark porting layer implementation for OpenVM
│   └── core_portme.h          # CoreMark porting layer definitions
├── coremark/                  # CoreMark sources (git submodule)
└── host/
    ├── Cargo.toml             # Host crate manifest
    ├── src/
    │   └── main.rs            # Host benchmark/prover entrypoint
    └── scripts/
        └── run_coremark.sh    # Wrapper to build and run the host harness
```

## Porting layer (`portme/`)

CoreMark expects a `core_portme.h`/`core_portme.c` implementation that supplies:
- **platform types/config** (`ee_*` typedefs, `SEED_METHOD`, `MEM_METHOD`, etc.)
- **timing hooks** (`start_time`, `stop_time`, `get_time`, `time_in_secs`)
- **printing** (`ee_printf`)

This repo’s `portme` has two notable features:

- **Printing is implemented via OpenVM**: `ee_printf` is implemented in C, but it routes each emitted byte through a small Rust-exported symbol (`coremark_putchar`) which calls `openvm::io::print`. The formatter is intentionally minimal and only supports the subset CoreMark uses (e.g. `%s`, `%d/%i`, `%u`, `%x/%X`, `%c`, `%%`, simple width/zero-padding like `%04x`, and `%lu`).
- **Timing is NOT implemented in-guest**: OpenVM guest programs don’t currently expose a meaningful wall-clock/cycle counter to the guest. We measure elapsed time using **host wall-clock** around `cargo openvm run`, and keep the CoreMark timing hooks as minimal stubs so the benchmark can run and print.

> [!WARNING]
>
> Because the timing hooks are stubbed, CoreMark will typically print:
> `ERROR! Must execute for at least 10 secs for a valid result!`
>
> This is expected in this repo’s current setup; use host wall-clock timing around
> `cargo openvm run` instead of the in-guest timing-derived fields (`Total ticks`,
> `Total time (secs)`, and `Iterations/Sec`), which will not be accurate.

## Guest program (`src/main.rs`)

- Uses `openvm::entry!(main)` to define the guest entrypoint.
- Exposes `coremark_putchar(u8)` for the C `ee_printf` implementation.
- Calls the C function `coremark_main(argc, argv)` (CoreMark’s C `main` renamed at build time) and returns `Ok(())` iff the return code is `0`.

## Host harness (`host/`)

The standalone host-side benchmark/proving binary lives under `host/`, separate from the guest crate at the repo root. The recommended entrypoint is:

```bash
./host/scripts/run_coremark.sh
```

The wrapper script builds the host binary from `host/`, runs it against the
guest ELF staged at `host/elf/openvm-coremark`, and enables some host-specific
features automatically based on the machine it is running on.

By default, it runs in `prove-stark` mode with the `maxperf` Cargo profile.
On `x86_64`, it also enables the host `rvr` execution feature. If `nvidia-smi` is
available, the script automatically enables CUDA and records GPU memory usage to
`gpu_memory_usage.csv`. If no NVIDIA tooling is available, the host harness
still runs without those profiling features.

### `run_coremark.sh` options

- `--mode <MODE>`: choose one of `execute`, `execute-metered`, `prove-app`, or `prove-stark`
- `--profile <PROFILE>`: build the host binary with `dev`, `release`, `maxperf`, or `profiling`
- `--cuda`: force CUDA acceleration instead of relying on auto-detection via `nvidia-smi`
- `--nsys`: run under NVIDIA Nsight Systems profiling; this implies CUDA and uses `sudo nsys profile`
- `--memcheck`: run under `compute-sanitizer --tool memcheck`
- `--synccheck`: run under `compute-sanitizer --tool synccheck`
- `--racecheck`: run under `compute-sanitizer --tool racecheck`

If you only want the standard host benchmark/prover flow, `./host/scripts/run_coremark.sh`
is enough. The CUDA, `compute-sanitizer`, and `nsys` paths are optional and only
needed for GPU acceleration or profiling/debugging work.

## Acknowledgements

- The zkVM framework uses [OpenVM](https://github.com/openvm-org/openvm)
- The benchmark workload directly uses [CoreMark](https://github.com/eembc/coremark)

## License

The code in this repository is licensed under MIT; see [LICENSE](LICENSE).

The bundled `coremark/` directory is third-party code from EEMBC's CoreMark project
and remains subject to its upstream license terms; see [coremark/LICENSE.md](coremark/LICENSE.md).
