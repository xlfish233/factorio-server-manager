#!/bin/sh
set -e

echo "Rustc Version: $(rustc --version)"
echo "Creating build..."
make gen_release
echo "Copying build artifacts from ${PWD}"
mkdir -p /build
cp -v build/factorio-server-manager-linux.zip /build/ || true
cp -v build/factorio-server-manager-windows.zip /build/ || true
