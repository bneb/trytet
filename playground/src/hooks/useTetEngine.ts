"use client";
import { useEffect, useRef, useState, useCallback } from 'react';

export function useTetEngine() {
  const workerRef = useRef<Worker | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Instantiate the dedicated WebWorker
    const worker = new Worker(new URL('../workers/tet-engine.ts', import.meta.url), {
      type: 'module',
    });
    
    workerRef.current = worker;

    worker.onmessage = (e: MessageEvent) => {
      if (e.data.type === 'INITIALIZED') {
        setInitialized(true);
      } else if (e.data.type === 'LOG') {
        setLogs((prev) => [...prev, String(e.data.data)]);
      } else if (e.data.type === 'ERROR') {
        setError(e.data.error);
      }
    };

    worker.postMessage({ type: 'INITIALIZE' });

    return () => {
      worker.terminate();
    };
  }, []);

  const hydrateSnapshot = useCallback((payload: Uint8Array) => {
    if (workerRef.current && initialized) {
      workerRef.current.postMessage({ type: 'HYDRATE', payload });
    } else {
      setError('Cannot hydrate: Engine not initialized');
    }
  }, [initialized]);

  return {
    initialized,
    logs,
    error,
    hydrateSnapshot,
  };
}
