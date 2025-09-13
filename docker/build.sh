#!/bin/bash
set -eou pipefail
# Build Docker image using multi-stage Rust Dockerfile
docker build -f Dockerfile -t fsmr:dev ..
