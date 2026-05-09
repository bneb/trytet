"use client";
import React, { useState, useEffect } from 'react';
import { Navbar } from '../../components/Navbar';
import { TrytetClient } from '@trytet/client';
import { Play, Loader2, Server, Hexagon } from 'lucide-react';
import { motion } from 'framer-motion';

// Generate 50 snippets: 40 valid, 10 infinite loops
const generateSnippets = () => {
    const snippets = [];
    for (let i = 0; i < 50; i++) {
        if (i % 5 === 0) {
            // Malicious/Hallucinated Loop
            snippets.push({
                type: 'loop' as const,
                code: `
                    let x = 0;
                    while(true) { x++; }
                    x;
                `
            });
        } else {
            // Valid Computation
            snippets.push({
                type: 'comp' as const,
                code: `
                    let sum = 0;
                    for (let j = 0; j < 1000; j++) {
                        sum += j;
                    }
                    "Result: " + sum;
                `
            });
        }
    }
    return snippets;
};

export default function BenchmarkDemo() {
  const [snippets] = useState(generateSnippets());
  const [isRunningNode, setIsRunningNode] = useState(false);
  const [isRunningTrytet, setIsRunningTrytet] = useState(false);
  const [nodeResults, setNodeResults] = useState<any[]>([]);
  const [trytetResults, setTrytetResults] = useState<any[]>([]);
  const [nodeDuration, setNodeDuration] = useState<number | null>(null);
  const [trytetDuration, setTrytetDuration] = useState<number | null>(null);
  const [wasmBytes, setWasmBytes] = useState<Uint8Array | null>(null);

  useEffect(() => {
    fetch('/js-evaluator.wasm')
      .then(res => res.arrayBuffer())
      .then(buffer => setWasmBytes(new Uint8Array(buffer)))
      .catch(err => console.error("Failed to load js-evaluator.wasm", err));
  }, []);

  const runNodeBenchmark = async () => {
    setIsRunningNode(true);
    setNodeResults([]);
    setNodeDuration(null);

    try {
        const baseUrl = process.env.NEXT_PUBLIC_TRYTET_API_URL || 'http://localhost:3000';
        const startTime = Date.now();

        // Evaluate all 50 snippets sequentially to show the wall-clock pain
        for (const item of snippets) {
            const startSnippet = Date.now();
            try {
                const res = await fetch(`${baseUrl}/v1/benchmark/node`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ 
                        snippet: item.code,
                        timeout_ms: 1000
                    })
                });
                
                const data = await res.json();
                const result = { ...data, type: item.type };
                setNodeResults(prev => [...prev, result]); // Update UI incrementally
            } catch (err) {
                console.error(err);
            }
        }
        
        setNodeDuration(Date.now() - startTime);
    } catch (err) {
        console.error(err);
    } finally {
        setIsRunningNode(false);
    }
  };

  const runTrytetBenchmark = async () => {
    if (!wasmBytes) return;
    setIsRunningTrytet(true);
    setTrytetResults([]);
    setTrytetDuration(null);

    try {
        const baseUrl = process.env.NEXT_PUBLIC_TRYTET_API_URL || 'http://localhost:3000';
        
        // 1. Initial "Warm-up" to ensure the js-evaluator is registered and JIT-compiled.
        await fetch(`${baseUrl}/v1/tet/execute`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                payload: Array.from(wasmBytes),
                alias: 'js-evaluator',
                allocated_fuel: 100000,
                max_memory_mb: 32,
            })
        });

        const startTime = Date.now();
        
        // 2. Evaluate all 50 snippets using the high-performance direct bridge
        for (const item of snippets) {
            const startSnippet = Date.now();
            try {
                const res = await fetch(`${baseUrl}/v1/cartridge/invoke`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        cartridge_id: 'js-evaluator',
                        payload: item.code,
                        fuel_limit: 5000000,
                        memory_limit_mb: 32
                    })
                });
                
                const data = await res.json();
                
                const result = {
                    status: data.status,
                    duration_ms: Date.now() - startSnippet,
                    fuel: data.fuel_consumed,
                    type: item.type
                };
                setTrytetResults(prev => [...prev, result]);
            } catch (err: any) {
                console.error(err);
            }
        }
        
        setTrytetDuration(Date.now() - startTime);
    } catch (err) {
        console.error(err);
    } finally {
        setIsRunningTrytet(false);
    }
  };

  const getStatusColorNode = (res: BenchmarkResult) => {
      if (res.status === 'Success') return 'bg-green-500';
      if (res.status === 'Timeout') return 'bg-red-500';
      return 'bg-gray-500';
  };

  const getStatusColorTrytet = (res: BenchmarkResult) => {
      if (res.status === 'Success') return 'bg-green-500';
      if (res.status === 'OutOfFuel') return 'bg-[var(--electric-blue)]'; // Fast fuel trap
      return 'bg-red-500';
  };

  return (
    <div className="flex flex-col min-h-screen relative bg-[var(--bg-main)] text-[var(--text-main)]">
      <Navbar />
      
      <main className="flex-1 w-full max-w-6xl mx-auto pt-24 px-6 pb-24">
        <div className="mb-12">
            <h1 className="text-4xl font-bold mb-4">Wall-Clock VM vs. <span className="text-[var(--electric-blue)]">Trytet Determinism</span></h1>
            <p className="text-lg text-[var(--text-sub)] max-w-3xl mb-6">
                Executing 50 LLM-generated JavaScript snippets (40 valid computations, 10 infinite loops). 
                Standard OS-level sandboxes (like Node VM or Docker) must rely on arbitrary Wall-Clock Timeouts. Trytet uses mathematical Fuel Metering to trap loops instantly.
            </p>
            <div className="flex gap-4">
                <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded px-4 py-3 flex items-center gap-3">
                    <Server className="text-gray-400" />
                    <div>
                        <div className="text-xs text-[var(--text-sub)]">System A</div>
                        <div className="font-bold text-sm">Node.js VM Module (1000ms Timeout)</div>
                    </div>
                </div>
                <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded px-4 py-3 flex items-center gap-3 border-[var(--electric-blue)]/50">
                    <Hexagon className="text-[var(--electric-blue)]" />
                    <div>
                        <div className="text-xs text-[var(--text-sub)]">System B</div>
                        <div className="font-bold text-sm text-[var(--electric-blue)]">Trytet Engine (5M Instruction Limit)</div>
                    </div>
                </div>
            </div>
        </div>
        
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          
          {/* Node VM Column */}
          <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg p-6 flex flex-col">
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-xl font-semibold flex items-center gap-2"><Server size={20}/> Node.js VM</h2>
                <button 
                    onClick={runNodeBenchmark} 
                    disabled={isRunningNode || isRunningTrytet}
                    className="bg-gray-700 hover:bg-gray-600 text-white font-bold py-2 px-4 rounded text-sm transition-colors flex items-center gap-2 disabled:opacity-50"
                >
                    {isRunningNode ? <Loader2 className="animate-spin" size={16} /> : <Play size={16} />}
                    {isRunningNode ? 'Running...' : 'Run Benchmark'}
                </button>
            </div>

            <div className="mb-6 flex gap-4 text-xs font-mono">
                <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-green-500"></div> Success</div>
                <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-red-500"></div> 1000ms Timeout</div>
            </div>

            <div className="flex-1 relative border border-[var(--card-border)] bg-[#0a0a0a] rounded-lg p-4 overflow-hidden flex flex-wrap content-start gap-[6px] min-h-[200px]">
                {nodeResults.map((res, i) => (
                    <motion.div
                        key={i}
                        initial={{ scale: 0, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        className={`w-6 h-6 rounded flex items-center justify-center text-[10px] font-bold ${getStatusColorNode(res)}`}
                        title={`Snippet ${i+1}: ${res.status} (${res.duration_ms}ms)`}
                    >
                        {res.type === 'loop' ? 'L' : 'C'}
                    </motion.div>
                ))}
            </div>

            <div className="mt-6 bg-[#0a0a0a] border border-[var(--card-border)] rounded p-4 flex justify-between items-center">
                <div className="text-[var(--text-sub)] text-sm">Total Execution Time:</div>
                <div className={`text-2xl font-mono font-bold ${nodeDuration ? 'text-red-400' : 'text-gray-500'}`}>
                    {nodeDuration ? `${(nodeDuration / 1000).toFixed(2)}s` : '0.00s'}
                </div>
            </div>
          </div>

          {/* Trytet Column */}
          <div className="bg-[var(--bg-sub)] border border-[var(--electric-blue)]/50 rounded-lg p-6 flex flex-col shadow-[0_0_15px_rgba(0,212,255,0.05)]">
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-xl font-semibold flex items-center gap-2 text-[var(--electric-blue)]"><Hexagon size={20}/> Trytet Wasm</h2>
                <button 
                    onClick={runTrytetBenchmark} 
                    disabled={isRunningNode || isRunningTrytet || !wasmBytes}
                    className="bg-[var(--electric-blue)] hover:bg-blue-600 text-white font-bold py-2 px-4 rounded text-sm transition-colors flex items-center gap-2 disabled:opacity-50"
                >
                    {isRunningTrytet ? <Loader2 className="animate-spin" size={16} /> : <Play size={16} />}
                    {isRunningTrytet ? 'Running...' : 'Run Benchmark'}
                </button>
            </div>

            <div className="mb-6 flex gap-4 text-xs font-mono">
                <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-green-500"></div> Success</div>
                <div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full bg-[var(--electric-blue)]"></div> Microsecond Fuel Trap</div>
            </div>

            <div className="flex-1 relative border border-[var(--card-border)] bg-[#0a0a0a] rounded-lg p-4 overflow-hidden flex flex-wrap content-start gap-[6px] min-h-[200px]">
                {trytetResults.map((res, i) => (
                    <motion.div
                        key={i}
                        initial={{ scale: 0, opacity: 0 }}
                        animate={{ scale: 1, opacity: 1 }}
                        className={`w-6 h-6 rounded flex items-center justify-center text-[10px] font-bold ${getStatusColorTrytet(res)}`}
                        title={`Snippet ${i+1}: ${res.status} (${res.duration_ms}ms, ${res.fuel} fuel)`}
                    >
                        {res.type === 'loop' ? 'L' : 'C'}
                    </motion.div>
                ))}
            </div>

            <div className="mt-6 bg-[#0a0a0a] border border-[var(--card-border)] rounded p-4 flex justify-between items-center">
                <div className="text-[var(--text-sub)] text-sm">Total Execution Time:</div>
                <div className={`text-2xl font-mono font-bold ${trytetDuration ? 'text-[var(--mint-success)]' : 'text-gray-500'}`}>
                    {trytetDuration ? `${(trytetDuration / 1000).toFixed(3)}s` : '0.000s'}
                </div>
            </div>
          </div>

        </div>
      </main>
    </div>
  );
}
 1000).toFixed(3)}s` : '0.000s'}
                </div>
            </div>
          </div>

        </div>
      </main>
    </div>
  );
}
