#!/usr/bin/env bash
set -e

echo "Building research-agent..."
cd ../../crates/research-agent
cargo build --target wasm32-wasip1 --release
cd ../../demos/swarm_demo

echo "Generating swarm manifest..."
cat > demo.toml <<EOF
[swarm]
name = "sovereign-research-swarm"

[[agents]]
alias = "manager"
base = "../../target/wasm32-wasip1/release/research-agent.wasm"
entrypoint = "_start"
[agents.economy]
max_fuel = 999999999
[agents.mesh]
allow_call = ["worker-*"]
EOF

echo "Appending 50 workers to manifest..."
for i in {1..50}; do
  cat >> demo.toml <<EOF

[[agents]]
alias = "worker-${i}"
base = "../../target/wasm32-wasip1/release/research-agent.wasm"
entrypoint = "_start"
[agents.economy]
max_fuel = 10000000
EOF
done

cd ../../

echo "Orchestrating Sovereign Swarm..."
TRYTET_API_URL=http://0.0.0.0:3000 cargo run --bin tet -- up demos/swarm_demo/demo.toml

echo ""
echo "Swarm booted. Demonstrating teleportation..."
TRYTET_API_URL=http://0.0.0.0:3000 cargo run --bin tet -- teleport worker-42 backup-node-alpha

echo ""
echo "Demo complete!"
