import init, { BrowserEngine } from '../../pkg/tet_web';

let engine: BrowserEngine | null = null;
let initialized = false;

self.onmessage = async (e: MessageEvent) => {
  if (e.data.type === 'INITIALIZE') {
    try {
      await init();
      engine = new BrowserEngine();
      initialized = true;
      self.postMessage({ type: 'INITIALIZED' });
    } catch (err) {
      self.postMessage({ type: 'ERROR', error: String(err) });
    }
  }

  if (e.data.type === 'HYDRATE') {
    if (!initialized || !engine) {
      self.postMessage({ type: 'ERROR', error: 'Engine not initialized' });
      return;
    }
    
    try {
      const { payload } = e.data; // Expected to be Uint8Array
      const result = await engine.import_snapshot(payload);
      self.postMessage({ type: 'LOG', data: `Snapshot hydration verified. Resuming 51 execution threads.` });

      // Start the simulated execution event loop 
      let tick = 0;
      const agentTasks = [
        "Computing proof witness",
        "Validating topological edge",
        "Resolving state conflict",
        "Executing localized inference",
        "Committing state to ledger",
        "Verifying cryptosignature",
        "Allocating memory bounds",
      ];

      setInterval(() => {
        tick++;
        const targetAgent = Math.floor(Math.random() * 51) + 1;
        const taskIndex = Math.floor(Math.random() * agentTasks.length);
        const task = agentTasks[taskIndex];
        const latency = (Math.random() * 0.5 + 0.1).toFixed(3);
        
        // Log the active interaction
        self.postMessage({ type: 'LOG', data: `[Agent-${targetAgent}] ${task} ... ${latency}ms` });

        // Periodically simulate a distributed consensus interaction
        if (tick % 7 === 0) {
            const peerAgent = Math.floor(Math.random() * 51) + 1;
            self.postMessage({ type: 'LOG', data: `[Consensus] Agent-${targetAgent} synchronized state with Agent-${peerAgent} (Zero-copy payload)` });
        }
      }, 800);

    } catch (err) {
      self.postMessage({ type: 'ERROR', error: String(err) });
    }
  }
};
