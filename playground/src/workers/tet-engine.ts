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
      self.postMessage({ type: 'LOG', data: result });
    } catch (err) {
      self.postMessage({ type: 'ERROR', error: String(err) });
    }
  }
};
