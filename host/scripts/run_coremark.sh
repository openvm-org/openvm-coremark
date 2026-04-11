#!/bin/bash
#
# Usage: ./host/scripts/run_coremark.sh [OPTIONS]
#
# Options:
#   --mode <MODE>       Set the mode (default: prove-stark)
#                       Valid modes: execute, execute-metered, prove-app, prove-stark
#   --profile <PROFILE> Set the Cargo build profile (default: release)
#                       Valid profiles: dev, release, profiling
#   --cuda              Force CUDA acceleration (auto-detected if nvidia-smi available)
#   --nsys              Run with nsys profiling and output summary stats
#   --<tool>            Run with compute-sanitizer --tool <tool> (memcheck, synccheck, racecheck)
#
# Examples:
#   ./host/scripts/run_coremark.sh                          # Prove coremark with STARK
#   ./host/scripts/run_coremark.sh --mode execute           # Execute only (no proof)
#   ./host/scripts/run_coremark.sh --cuda                   # Force CUDA acceleration
#   ./host/scripts/run_coremark.sh --nsys                   # Run with nsys profiling
#
set -e

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
HOST_DIR=$(cd -- "$SCRIPT_DIR/.." && pwd)
REPO_ROOT=$(cd -- "$HOST_DIR/.." && pwd)
WORKDIR=$REPO_ROOT

ELF="$HOST_DIR/elf/openvm-coremark"
if [ ! -f "$ELF" ]; then
    echo "Error: coremark ELF not found at $ELF" >&2
    echo "Copy it manually: cp <path-to-coremark-elf> $ELF" >&2
    exit 1
fi

# =============== GPU memory usage monitoring ============================
source "$SCRIPT_DIR/gpu_monitor.sh"
GPU_LOG_FILE="$WORKDIR/gpu_memory_usage.csv"
trap finalize_gpu_monitor EXIT

NVIDIA_SMI_READY=false
if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
    NVIDIA_SMI_READY=true
fi

# Parse command-line arguments
MODE_OVERRIDE=""
PROFILE_OVERRIDE=""
USE_CUDA=false
CUDA_REASON=""
USE_NSYS=false
COMPUTE_SANITIZER_ARGS=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --mode)
            MODE_OVERRIDE="$2"
            shift 2
            ;;
        --profile)
            PROFILE_OVERRIDE="$2"
            shift 2
            ;;
        --cuda)
            USE_CUDA=true
            CUDA_REASON="requested via --cuda script argument"
            shift
            ;;
        --nsys)
            USE_NSYS=true
            USE_CUDA=true
            CUDA_REASON="requested via --nsys script argument"
            shift
            ;;
        --memcheck)
            COMPUTE_SANITIZER_ARGS="compute-sanitizer --tool memcheck"
            shift
            ;;
        --synccheck)
            COMPUTE_SANITIZER_ARGS="compute-sanitizer --tool synccheck"
            shift
            ;;
        --racecheck)
            COMPUTE_SANITIZER_ARGS="compute-sanitizer --tool racecheck"
            shift
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

if [ "$USE_CUDA" = "false" ] && [ "$NVIDIA_SMI_READY" = "true" ]; then
    USE_CUDA=true
    CUDA_REASON="nvidia-smi detected a CUDA-capable GPU"
fi

if [ "$USE_CUDA" = "true" ]; then
    echo "Using CUDA acceleration ($CUDA_REASON)."
fi

if [ "$NVIDIA_SMI_READY" = "true" ] && [ "$USE_NSYS" = "false" ]; then
    start_gpu_monitor "$GPU_LOG_FILE" "$GPU_MONITOR_INTERVAL"
elif [ "$USE_NSYS" = "true" ]; then
    echo "GPU memory monitoring disabled for nsys profiling."
else
    echo "nvidia-smi not detected; GPU memory monitoring disabled."
fi

MODE="${MODE_OVERRIDE:-prove-stark}"

# Map profile aliases and set target directory
case "${PROFILE_OVERRIDE:-release}" in
    dev|debug)
        PROFILE="dev"
        TARGET_DIR="debug"
        ;;
    release)
        PROFILE="release"
        TARGET_DIR="release"
        ;;
    *)
        PROFILE="${PROFILE_OVERRIDE:-profiling}"
        TARGET_DIR="$PROFILE"
        ;;
esac

FEATURES="parallel,metrics,jemalloc,unprotected"
BIN_NAME="openvm-coremark-benchmark"
MAX_SEGMENT_LENGTH=$((1 << 22))
segment_max_memory=$((15 << 30))
export VPMM_PAGE_SIZE=$((4 << 20))
if [[ -z "${VPMM_PAGES:-}" ]] && [[ "$MODE" == "prove-stark" || "$MODE" == "prove-app" ]]; then
    export VPMM_PAGES=$((16 << 8))
fi

if [ "$USE_CUDA" = "true" ]; then
    FEATURES="$FEATURES,cuda"
fi
if [ "$USE_NSYS" = "true" ]; then
    FEATURES="$FEATURES,nvtx"
fi

arch=$(uname -m)
case $arch in
arm64|aarch64)
    RUSTFLAGS="-Ctarget-cpu=native"
    ;;
x86_64|amd64)
    RUSTFLAGS="-Ctarget-cpu=native"
    FEATURES="$FEATURES,aot"
    ;;
*)
    echo "Unsupported architecture: $arch"
    exit 1
    ;;
esac

if [ "$USE_NSYS" = "false" ]; then
    export JEMALLOC_SYS_WITH_MALLOC_CONF="retain:true,background_thread:true,metadata_thp:always,dirty_decay_ms:10000,muzzy_decay_ms:10000,abort_conf:true"
fi

MANIFEST_PATH="$HOST_DIR/Cargo.toml"

if [[ "${OPENVM_BENCH_SKIP_BUILD:-0}" != "1" ]]; then
    RUSTFLAGS=$RUSTFLAGS cargo build --manifest-path "$MANIFEST_PATH" --target-dir "$REPO_ROOT/target" --bin $BIN_NAME --profile=$PROFILE --no-default-features --features=$FEATURES
fi

BIN=$REPO_ROOT/target/$TARGET_DIR/$BIN_NAME

BIN_ARGS="--mode $MODE \
--max-segment-length $MAX_SEGMENT_LENGTH \
--segment-max-memory $segment_max_memory"

export RUST_LOG="info,p3_=warn"

if [ "$USE_NSYS" = "true" ]; then
    NSYS_OUTPUT="coremark.nsys-rep"
    NSYS_ARGS="--trace=cuda,nvtx --cuda-memory-usage=true --force-overwrite=true -o $NSYS_OUTPUT"

    echo "[sudo] Running with nsys profiling..."
    sudo env PATH="$PATH" HOME="$HOME" RUST_LOG="$RUST_LOG" \
         VPMM_PAGE_SIZE="${VPMM_PAGE_SIZE:-}" VPMM_PAGES="${VPMM_PAGES:-}" \
         LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
         nsys profile $NSYS_ARGS --gpu-metrics-devices=all \
         $BIN $BIN_ARGS

    echo "=== CUDA GPU Kernel Summary ==="
    nsys stats --force-export=true --report cuda_gpu_kern_sum "$NSYS_OUTPUT"
    echo "=== CUDA Memory Time Summary ==="
    nsys stats --force-export=true --report cuda_gpu_mem_time_sum "$NSYS_OUTPUT"
    echo "=== CUDA Memory Size Summary ==="
    nsys stats --force-export=true --report cuda_gpu_mem_size_sum "$NSYS_OUTPUT"
else
    export OUTPUT_PATH="metrics.json"
    $COMPUTE_SANITIZER_ARGS $BIN $BIN_ARGS
fi
