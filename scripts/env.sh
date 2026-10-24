# Source this before building on a Mac where Homebrew's cargo shadows rustup's.
# The rustup toolchain carries the wasm32-unknown-unknown target; Homebrew's does not.
if [ -d /usr/local/opt/rustup/bin ]; then
  export PATH="/usr/local/opt/rustup/bin:$PATH"
fi
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"   # trunk, sqlx-cli, agy
