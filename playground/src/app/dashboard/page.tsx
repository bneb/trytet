"use client";
import React, { useState } from 'react';
import { Navbar } from '../../components/Navbar';
import { TrytetClient, TetExecutionRequest, TetExecutionResult } from '@trytet/client';

export default function Dashboard() {
  const [wasmFile, setWasmFile] = useState<File | null>(null);
  const [fuel, setFuel] = useState<number>(10000000);
  const [result, setResult] = useState<TetExecutionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleExecute = async () => {
    if (!wasmFile) {
      setError("Please select a .wasm file.");
      return;
    }

    setLoading(true);
    setError('');
    setResult(null);

    try {
      const arrayBuffer = await wasmFile.arrayBuffer();
      const payload = Array.from(new Uint8Array(arrayBuffer));

      const client = new TrytetClient({ baseUrl: process.env.NEXT_PUBLIC_TRYTET_API_URL || 'http://localhost:3000' });
      const request: TetExecutionRequest = {
        payload,
        allocated_fuel: fuel,
        max_memory_mb: 64,
        alias: 'dashboard-agent',
      };

      const res = await client.execute(request);
      setResult(res);
    } catch (err: unknown) {
      if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col min-h-screen relative bg-[var(--bg-main)] text-[var(--text-main)]">
      <Navbar />
      
      <main className="flex-1 w-full max-w-4xl mx-auto pt-24 px-6">
        <h1 className="text-3xl font-bold mb-8 text-[var(--electric-blue)]">Trytet Engine Dashboard</h1>
        
        <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg p-6 mb-8">
          <h2 className="text-xl font-semibold mb-4">Execute Sovereign Agent</h2>
          
          <div className="flex flex-col gap-4 max-w-md">
            <div>
              <label className="block text-sm font-medium mb-1 text-[var(--text-sub)]">Agent Payload (.wasm)</label>
              <input 
                type="file" 
                accept=".wasm" 
                onChange={(e) => setWasmFile(e.target.files?.[0] || null)}
                className="w-full text-sm file:mr-4 file:py-2 file:px-4 file:rounded file:border-0 file:text-sm file:font-semibold file:bg-[var(--electric-blue)] file:text-white hover:file:bg-blue-600"
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-1 text-[var(--text-sub)]">Fuel Budget (Instructions)</label>
              <input 
                type="number" 
                value={fuel} 
                onChange={(e) => setFuel(Number(e.target.value))}
                className="w-full bg-[var(--bg-main)] border border-[var(--card-border)] rounded px-3 py-2 text-sm focus:outline-none focus:border-[var(--electric-blue)]"
              />
            </div>

            <button 
              onClick={handleExecute} 
              disabled={loading || !wasmFile}
              className="mt-4 bg-[var(--electric-blue)] text-white font-bold py-2 px-4 rounded disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? 'Executing...' : 'Boot Agent'}
            </button>

            {error && (
              <div className="mt-4 p-3 bg-red-900/30 border border-red-500/50 rounded text-red-200 text-sm">
                {error}
              </div>
            )}
          </div>
        </div>

        {result && (
          <div className="bg-[var(--bg-sub)] border border-[var(--card-border)] rounded-lg p-6">
            <h2 className="text-xl font-semibold mb-4">Execution Results</h2>
            
            <div className="grid grid-cols-2 gap-4 mb-6">
              <div className="bg-[var(--bg-main)] p-4 rounded border border-[var(--card-border)]">
                <div className="text-[var(--text-sub)] text-xs mb-1">Status</div>
                <div className={`font-mono ${result.status === 'Success' ? 'text-[var(--mint-success)]' : 'text-[var(--magenta-teleport)]'}`}>
                  {typeof result.status === 'string' ? result.status : JSON.stringify(result.status)}
                </div>
              </div>
              <div className="bg-[var(--bg-main)] p-4 rounded border border-[var(--card-border)]">
                <div className="text-[var(--text-sub)] text-xs mb-1">Tet ID</div>
                <div className="font-mono text-sm truncate">{result.tet_id}</div>
              </div>
              <div className="bg-[var(--bg-main)] p-4 rounded border border-[var(--card-border)]">
                <div className="text-[var(--text-sub)] text-xs mb-1">Fuel Consumed</div>
                <div className="font-mono text-sm">{result.fuel_consumed.toLocaleString()}</div>
              </div>
              <div className="bg-[var(--bg-main)] p-4 rounded border border-[var(--card-border)]">
                <div className="text-[var(--text-sub)] text-xs mb-1">Duration</div>
                <div className="font-mono text-sm">{result.execution_duration_us} µs</div>
              </div>
            </div>

            <div>
              <h3 className="text-sm font-semibold mb-2 text-[var(--text-sub)]">STDOUT</h3>
              <pre className="bg-[#0a0a0a] text-gray-300 p-4 rounded border border-[var(--card-border)] overflow-x-auto text-xs whitespace-pre-wrap">
                {result.telemetry.stdout_lines.length > 0 ? result.telemetry.stdout_lines.join('\n') : '(empty)'}
              </pre>
            </div>
            
            <div className="mt-4">
              <h3 className="text-sm font-semibold mb-2 text-[var(--text-sub)]">STDERR</h3>
              <pre className="bg-[#0a0a0a] text-red-400 p-4 rounded border border-[var(--card-border)] overflow-x-auto text-xs whitespace-pre-wrap">
                {result.telemetry.stderr_lines.length > 0 ? result.telemetry.stderr_lines.join('\n') : '(empty)'}
              </pre>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
