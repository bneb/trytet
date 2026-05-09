"use client";
import React, { useState, useEffect } from 'react';
import { Navbar } from '../../components/Navbar';
import { TrytetClient, TetExecutionResult } from '@trytet/client';
import { Play, Layers, Zap, Loader2 } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

export default function MctsDemo() {
  const [isRunning, setIsRunning] = useState(false);
  const [snapshotId, setSnapshotId] = useState<string | null>(null);
  const [results, setResults] = useState<TetExecutionResult[]>([]);
  const [wasmBytes, setWasmBytes] = useState<Uint8Array | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    fetch('/mcts-guest.wasm')
      .then(res => res.arrayBuffer())
      .then(buffer => setWasmBytes(new Uint8Array(buffer)))
      .catch(err => console.error("Failed to load mcts-guest.wasm", err));
  }, []);

  const addLog = (msg: string) => {
    setLogs(prev => [...prev, `[${new Date().toISOString().split('T')[1].slice(0, -1)}] ${msg}`]);
  };

  const initBaseState = async () => {
    if (!wasmBytes) return;
    setIsRunning(true);
    setLogs([]);
    setResults([]);
    setProgress(0);
    
    addLog("Initializing 5MB base context (e.g. loading heavy AST or model weights)...");

    try {
        const client = new TrytetClient({ baseUrl: process.env.NEXT_PUBLIC_TRYTET_API_URL || 'http://localhost:3000' });
        
        const res = await client.execute({
            payload: Array.from(wasmBytes),
            allocated_fuel: 50_000_000,
            max_memory_mb: 64,
            alias: 'mcts-base',
            env: { PERMUTATION_CODE: 'BASE_INIT' }
        });
        
        addLog(`Base agent execution complete in ${res.execution_duration_us / 1000}ms.`);
        addLog(`Taking exact linear memory snapshot...`);
        
        const snap = await client.snapshot(res.tet_id);
        setSnapshotId(snap.snapshot_id);
        
        addLog(`Snapshot [${snap.snapshot_id}] created. Size: ${(snap.size_bytes / 1024 / 1024).toFixed(2)} MB.`);
        addLog(`Ready to branch execution timeline.`);
    } catch (err: unknown) {
        if (err instanceof Error) {
            addLog(`Error: ${err.message}`);
        } else {
            addLog(`Error: ${String(err)}`);
        }
    } finally {
        setIsRunning(false);
    }
  };

  const runForks = async (count: number) => {
    if (!snapshotId || !wasmBytes) return;
    setIsRunning(true);
    setResults([]);
    setProgress(0);
    addLog(`Initiating ${count} concurrent branches via O(1) memory forking...`);
    
    const client = new TrytetClient({ baseUrl: process.env.NEXT_PUBLIC_TRYTET_API_URL || 'http://localhost:3000' });
    
    const startTime = Date.now();
    const batchSize = 50; // Throttle to avoid maxing browser socket pool
    let completed = 0;
    
    for (let i = 0; i < count; i += batchSize) {
        const promises = [];
        const batch = Math.min(batchSize, count - i);
        
        for (let j = 0; j < batch; j++) {
            // Distribute permutations to show robustness
            let code = 'SUCCESS';
            const rand = Math.random();
            if (rand < 0.1) code = 'INFINITE_LOOP';
            else if (rand < 0.15) code = 'MEMORY_BOMB';
            else if (rand < 0.2) code = 'CRASH';
            
            promises.push(
                client.fork(snapshotId, {
                    payload: Array.from(wasmBytes), // Provide payload to fork
                    allocated_fuel: 1_000_000,
                    max_memory_mb: 64,
                    env: { PERMUTATION_CODE: code }
                }).then(res => {
                    setResults(prev => [...prev, res]);
                    completed++;
                    setProgress(Math.round((completed / count) * 100));
                }).catch(e => {
                    console.error("Fork failed", e);
                    completed++;
                })
            );
        }
        await Promise.all(promises);
    }
    
    const duration = Date.now() - startTime;
    addLog(`Evaluated ${count} code permutations simultaneously in ${duration}ms.`);
    setIsRunning(false);
  };

  const getStatusColor = (status: unknown) => {
      if (status === 'Success') return 'bg-green-500';
      if (status === 'OutOfFuel') return 'bg-yellow-500';
      if (status === 'MemoryExceeded') return 'bg-purple-500';
      if (typeof status === 'object' && status !== null && 'Crash' in status) return 'bg-red-500';
      return 'bg-gray-500';
  };

  const getStatusText = (status: unknown) => {
      if (status === 'Success') return 'Syntactically Valid';
      if (status === 'OutOfFuel') return 'Infinite Loop Trapped';
      if (status === 'MemoryExceeded') return 'Memory Bomb Prevented';
      if (typeof status === 'object' && status !== null && 'Crash' in status) return 'AST Logic Crash';
      return 'Unknown';
  };

  return (
    <div className="flex flex-col min-h-screen relative bg-[var(--bg-main)] text-[var(--text-main)]">
      <Navbar />
      
      <main className="flex-1 w-full max-w-6xl mx-auto pt-24 px-6 pb-24">
        <div className="mb-12">
            <h1 className="text-4xl font-bold mb-4">Multi-Path <span className="text-[var(--electric-blue)]">MCTS</span> Evaluation</h1>
            <p className="text-lg text-[var(--text-sub)] max-w-3xl">
                To write complex code, agents must evaluate hundreds of trajectories (Monte Carlo Tree Search). Docker takes 3 seconds to boot one container. Trytet forks a 5MB agent memory state in O(1) time, evaluating 1,000 code permutations in milliseconds.
            </p>
        </div>
        
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-8">
          
          {/* Controls Column */}
          <div className="col-span-4 flex flex-col gap-6">
            <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg p-6">
                <h2 className="text-xl font-semibold mb-4">1. Initialize Base State</h2>
                <p className="text-sm text-[var(--text-sub)] mb-6">
                    Launch the core agent and load heavy context (e.g. 5MB of ASTs/weights) into linear memory. We will snapshot this exact boundary.
                </p>

                <button 
                    onClick={initBaseState} 
                    disabled={isRunning || !wasmBytes}
                    className={`w-full font-bold py-3 px-4 rounded-md flex items-center justify-center gap-2 transition-all ${!snapshotId ? 'bg-[var(--electric-blue)] hover:bg-blue-600 text-white' : 'bg-transparent border border-[var(--card-border)] text-[var(--text-sub)]'} disabled:opacity-50 disabled:cursor-not-allowed`}
                >
                    {isRunning && !snapshotId ? <Loader2 className="animate-spin" size={18} /> : <Play size={18} />}
                    {snapshotId ? 'Base State Loaded' : 'Initialize & Snapshot'}
                </button>
            </div>

            <div className={`bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg p-6 transition-opacity duration-500 ${snapshotId ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
                <h2 className="text-xl font-semibold mb-4">2. Parallel Branching</h2>
                <p className="text-sm text-[var(--text-sub)] mb-6">
                    Fork the baseline memory into completely isolated sub-sandboxes to evaluate generated code paths. Malicious paths are trapped via fuel metering.
                </p>

                <div className="flex gap-4">
                    <button 
                        onClick={() => runForks(100)} 
                        disabled={isRunning}
                        className="flex-1 font-bold py-3 px-4 rounded-md border border-[var(--electric-blue)] text-[var(--electric-blue)] hover:bg-[var(--electric-blue)] hover:text-white transition-all disabled:opacity-50"
                    >
                        100 Paths
                    </button>
                    <button 
                        onClick={() => runForks(500)} 
                        disabled={isRunning}
                        className="flex-1 font-bold py-3 px-4 rounded-md bg-[var(--electric-blue)] text-white hover:bg-blue-600 transition-all disabled:opacity-50"
                    >
                        500 Paths
                    </button>
                </div>
            </div>

            {/* Terminal Output */}
            <div className="bg-[#0a0a0a] border border-[var(--card-border)] rounded-lg flex flex-col overflow-hidden h-64">
                <div className="bg-[var(--bg-sub)] px-4 py-2 border-b border-[var(--card-border)] flex items-center gap-2">
                    <span className="text-xs text-[var(--text-sub)] font-mono">Engine Console</span>
                </div>
                <div className="p-4 flex-1 overflow-y-auto font-mono text-[11px] leading-relaxed whitespace-pre-wrap flex flex-col gap-1">
                    {logs.map((log, i) => (
                        <div key={i} className="text-gray-400">{log}</div>
                    ))}
                </div>
            </div>
          </div>

          {/* Visualization Column */}
          <div className="col-span-8">
            <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg h-full p-6 flex flex-col">
                <div className="flex justify-between items-end mb-6">
                    <div>
                        <h2 className="text-xl font-semibold mb-1">Execution Topology</h2>
                        <div className="text-sm text-[var(--text-sub)]">Simulating Sub-Millisecond Wasm Iterations</div>
                    </div>
                    <div className="flex gap-4 text-xs font-mono">
                        <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-green-500"></div> Valid</div>
                        <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-yellow-500"></div> Inf Loop</div>
                        <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-red-500"></div> Crash</div>
                    </div>
                </div>

                <div className="flex-1 relative border border-[var(--card-border)] bg-[#0a0a0a] rounded-lg p-4 overflow-hidden flex flex-wrap content-start gap-[2px]">
                    {results.length === 0 && !isRunning && (
                        <div className="absolute inset-0 flex items-center justify-center text-[var(--text-sub)]">
                            Awaiting execution...
                        </div>
                    )}
                    
                    {isRunning && results.length === 0 && (
                        <div className="absolute inset-0 flex items-center justify-center text-[var(--electric-blue)]">
                            <Loader2 className="animate-spin w-8 h-8" />
                        </div>
                    )}

                    {results.map((res, i) => (
                        <motion.div
                            key={i}
                            initial={{ scale: 0, opacity: 0 }}
                            animate={{ scale: 1, opacity: 1 }}
                            className={`w-2 h-2 md:w-3 md:h-3 rounded-sm ${getStatusColor(res.status)}`}
                            title={getStatusText(res.status)}
                        />
                    ))}
                </div>

                {isRunning && results.length > 0 && (
                    <div className="mt-4 flex items-center gap-4">
                        <div className="flex-1 h-1 bg-[var(--bg-main)] rounded-full overflow-hidden">
                            <div className="h-full bg-[var(--electric-blue)] transition-all duration-200" style={{ width: `${progress}%` }}></div>
                        </div>
                        <div className="text-xs font-mono text-[var(--text-sub)]">{progress}%</div>
                    </div>
                )}
                
                {results.length > 0 && !isRunning && (
                    <div className="mt-6 grid grid-cols-4 gap-4">
                        <div className="bg-[var(--bg-main)] border border-[var(--card-border)] rounded p-3">
                            <div className="text-[10px] uppercase text-[var(--text-sub)] tracking-wider mb-1">Evaluated Paths</div>
                            <div className="text-xl font-bold">{results.length}</div>
                        </div>
                        <div className="bg-[var(--bg-main)] border border-[var(--card-border)] rounded p-3">
                            <div className="text-[10px] uppercase text-[var(--text-sub)] tracking-wider mb-1">Valid ASTs</div>
                            <div className="text-xl font-bold text-green-500">{results.filter(r => r.status === 'Success').length}</div>
                        </div>
                        <div className="bg-[var(--bg-main)] border border-[var(--card-border)] rounded p-3">
                            <div className="text-[10px] uppercase text-[var(--text-sub)] tracking-wider mb-1">Trapped Loops</div>
                            <div className="text-xl font-bold text-yellow-500">{results.filter(r => r.status === 'OutOfFuel').length}</div>
                        </div>
                        <div className="bg-[var(--bg-main)] border border-[var(--card-border)] rounded p-3">
                            <div className="text-[10px] uppercase text-[var(--text-sub)] tracking-wider mb-1">Crashes Isolated</div>
                            <div className="text-xl font-bold text-red-500">{results.filter(r => typeof r.status === 'object' && r.status.Crash).length}</div>
                        </div>
                    </div>
                )}
            </div>
          </div>

        </div>
      </main>
    </div>
  );
}
