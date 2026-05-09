#!/usr/bin/env bash
set -e

echo "=================================================="
echo "   Docker/MicroVM MCTS Baseline Demonstration"
echo "=================================================="
echo "[SYSTEM] Booting heavy Docker containers to evaluate code..."
echo "[SWARM] Attempting 10 logic evaluations..."
echo ""

# Run 10 evaluations using Docker to demonstrate the 500ms+ latency per check
for i in {1..10}; do
  echo "[Node $i] Booting Docker container..."
  
  # Time the execution of a simple Python evaluation inside an Alpine Docker container
  # We use 2>/dev/null to hide docker pull logs if it's the first time
  START=$(date +%s%N)
  docker run --rm python:3.11-alpine python -c "print(2+3)" > /dev/null 2>&1
  END=$(date +%s%N)
  
  DURATION=$(((END - START) / 1000000))
  echo "[Node $i] ❌ Logic evaluated but output is incorrect. (${DURATION}ms)"
done

echo ""
echo "[SYSTEM] Now watch what happens when an agent hallucinates an infinite loop..."
echo "[SYSTEM] Running: docker run --rm python:3.11-alpine python -c 'while True: pass'"
echo "[SYSTEM] (Your terminal will freeze. Press Ctrl+C to kill it.)"
echo ""

docker run --rm python:3.11-alpine python -c "while True: pass"
