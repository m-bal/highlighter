#!/bin/bash
set -e

build() {
  echo "Building silicon.nvim from source..."

  cargo build --release --target-dir ./target

  # Place the compiled library where Neovim can find it.
  mkdir -p lua

  if [ "$(uname)" == "Darwin" ]; then
    mv target/release/libhighlighter.dylib lua/highlighter.so
  elif [ "$(expr substr $(uname -s) 1 5)" == "Linux" ]; then
    mv target/release/libhighlighter.so lua/highlighter.so
  elif [ "$(expr substr $(uname -s) 1 10)" == "MINGW64_NT" ]; then
    mv target/release/highlighter.dll lua/highlighter.dll
  fi
}

build
