#!/bin/bash

DIR="$(dirname "$0")"

if cargo "$@"; then
    [ -d "$DIR/target/debug" ] && cp "$DIR/target/debug/libhighlighter.dylib" "lua/highlighter.so"
    [ -d "$DIR/target/release" ] && cp "$DIR/target/release/libhighlighter.dylib" "lua/highlighter.so"
fi
