# coremark-openvm

Run the [CoreMark](coremark/README.md) benchmark as an OpenVM guest program.

## Repo structure

- `coremark/`: CoreMark sources (submodule snapshot)
- `portme/`: CoreMark “porting layer” (`core_portme.{c,h}`) for OpenVM
- `src/main.rs`: OpenVM guest entrypoint that calls into CoreMark
- `host/`: host-side benchmark/prover harness for running and verifying the guest ELF
- `build.rs`: Builds CoreMark C sources into a static library for the Rust crate
- `scripts/run_coremark.sh`: convenience wrapper for the host benchmark harness
- `openvm.toml`: OpenVM app configuration (RV32IM + IO enabled)

## Porting layer (`portme/`)

CoreMark expects a `core_portme.h`/`core_portme.c` implementation that supplies:
- **platform types/config** (`ee_*` typedefs, `SEED_METHOD`, `MEM_METHOD`, etc.)
- **timing hooks** (`start_time`, `stop_time`, `get_time`, `time_in_secs`)
- **printing** (`ee_printf`)

This repo’s `portme` has two notable features:

- **Printing is implemented via OpenVM**: `ee_printf` is implemented in C, but it routes each emitted byte through a small Rust-exported symbol (`coremark_putchar`) which calls `openvm::io::print`. The formatter is intentionally minimal and only supports the subset CoreMark uses (e.g. `%s`, `%d/%i`, `%u`, `%x/%X`, `%c`, `%%`, simple width/zero-padding like `%04x`, and `%lu`).
- **Timing is NOT implemented in-guest**: OpenVM guest programs don’t currently expose a meaningful wall-clock/cycle counter to the guest. We measure elapsed time using **host wall-clock** around `cargo openvm run`, and keep the CoreMark timing hooks as minimal stubs so the benchmark can run and print.

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

The standalone host-side benchmark/proving binary lives under `host/`, separate from the guest crate at the repo root. Run it via:

```bash
./scripts/run_coremark.sh
```

## Building/running with `cargo openvm`

### Install `cargo openvm`

See [the official OpenVM docs](https://docs.openvm.dev/book/getting-started/introduction) for installation instructions.

### Commands

- Build the guest:

```bash
cargo openvm build
```

- Run the guest:

```bash
cargo openvm run
```

### Setting a fixed iteration count

CoreMark iterations are controlled via the C macro `ITERATIONS` (default `0` = auto-calibrate). You can override it by passing a C define during the build, e.g.:

```bash
CFLAGS="-DITERATIONS=1000" cargo openvm run
```
