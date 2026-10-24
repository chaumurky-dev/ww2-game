# Source this before building on a Mac where Homebrew's cargo shadows rustup's.
# The rustup toolchain carries the wasm32-unknown-unknown target; Homebrew's does not.
if [ -d /usr/local/opt/rustup/bin ]; then
  export PATH="/usr/local/opt/rustup/bin:$PATH"
fi
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"   # trunk, sqlx-cli, agy
# trunk's wasm link step (rust-lld) needs the toolchain's libLLVM on this Mac.
export DYLD_FALLBACK_LIBRARY_PATH="$HOME/.rustup/toolchains/stable-x86_64-apple-darwin/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
