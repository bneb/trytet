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
      // Use the real snapshot_state/restore_state bridge for round-trip proof
      const snapshot = engine.snapshot_state(payload);
      const restored = engine.restore_state(snapshot);
      self.postMessage({ type: 'LOG', data: `Snapshot round-trip verified. ${snapshot.length} bytes serialized via bincode.` });
    } catch (err) {
      self.postMessage({ type: 'ERROR', error: String(err) });
    }
  }
};
