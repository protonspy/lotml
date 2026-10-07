#!/usr/bin/env bash
# The guide's training run inside a RunPod pod, at the commit being run (specs/training-pipeline/
# R1.6): uv, the Rust toolchain the compiler pins and the release compiler, the training
# environment on uv's own CPython 3.13.14 (the image's 3.13 breaks torch's import), llama.cpp's
# release and source checked by digest, then the stages. The token is held aside while installers
# and builds run and given back to the stages alone. The pod's wrapper
# (harness/lotml_harness/guide/pod.py) holds it to its deadline and removes the pod after.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export UV_PYTHON_PREFERENCE=only-managed UV_PYTHON=3.13.14
token="$HF_TOKEN"
unset HF_TOKEN

curl --proto '=https' --tlsv1.2 -LsSf https://astral.sh/uv/0.11.29/install.sh | sh
toolchain="$(sed -n 's/^rust-version = "\(.*\)"/\1/p' "$root/compiler/Cargo.toml")"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain "$toolchain"
cargo build --manifest-path "$root/compiler/Cargo.toml" --locked --release -p lotml
uv --directory "$root/harness" sync --locked --group train --group guide

llama=/llama
mkdir -p "$llama"
fetch() {
  curl --proto '=https' --tlsv1.2 -fsSL -o "$1" "$2"
  echo "$3  $1" | sha256sum -c --quiet -
  tar -xzf "$1" -C "$llama"
}
release="https://github.com/ggml-org/llama.cpp/releases/download/$LOTML_LLAMA"
fetch "$llama/bin.tar.gz" "$release/llama-$LOTML_LLAMA-bin-ubuntu-cuda-12.8-x64.tar.gz" \
  "$LOTML_LLAMA_BINARY_SHA256"
fetch "$llama/cudart.tar.gz" "$release/cudart-llama-$LOTML_LLAMA-bin-ubuntu-cuda-12.8-x64.tar.gz" \
  "$LOTML_LLAMA_CUDART_SHA256"
fetch "$llama/src.tar.gz" \
  "https://github.com/ggml-org/llama.cpp/archive/refs/tags/$LOTML_LLAMA.tar.gz" \
  "$LOTML_LLAMA_SOURCE_SHA256"
export PYTHONPATH="$llama/llama.cpp-$LOTML_LLAMA/gguf-py"

HF_TOKEN="$token" exec uv --directory "$root/harness" run --locked --group train --group guide \
  python -m lotml_harness.guide.stages --llama-cpp "$llama"
