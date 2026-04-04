//! Console — Phase 27.1
//!
//! An embedded, zero-dependency web dashboard served from the engine binary.
//! Uses `include_str!` to embed the HTML/JS/CSS into the binary at compile time,
//! requiring no internet connection to view the dashboard.
//!
//! Pipes `/v1/swarm/metrics` directly into a real-time auto-refreshing UI.

use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

/// The embedded single-page dashboard HTML.
///
/// This is a self-contained HTML file with inline CSS/JS that polls
/// the `/v1/swarm/metrics` endpoint every 2 seconds and renders
/// a live Northstar report with animated gauges.
const CONSOLE_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Trytet Console — Control Plane</title>
<style>
  :root {
    --bg: #0a0a0f;
    --surface: #12121a;
    --border: #1e1e2e;
    --text: #e0e0e8;
    --dim: #6b6b80;
    --accent: #00d4ff;
    --green: #00ff88;
    --red: #ff4466;
    --orange: #ffaa00;
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    background: var(--bg);
    color: var(--text);
    font-family: 'SF Mono', 'Cascadia Code', 'Fira Code', monospace;
    min-height: 100vh;
    padding: 2rem;
  }
  h1 {
    font-size: 1.6rem;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    color: var(--accent);
    margin-bottom: 0.5rem;
  }
  .subtitle {
    color: var(--dim);
    font-size: 0.85rem;
    margin-bottom: 2rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 1.5rem;
    margin-bottom: 2rem;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 1.5rem;
    transition: border-color 0.3s ease;
  }
  .card:hover { border-color: var(--accent); }
  .card-label {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--dim);
    margin-bottom: 0.5rem;
  }
  .card-value {
    font-size: 2rem;
    font-weight: 700;
    color: var(--accent);
  }
  .card-unit {
    font-size: 0.85rem;
    color: var(--dim);
    margin-left: 0.25rem;
  }
  .card-status {
    font-size: 0.85rem;
    margin-top: 0.5rem;
  }
  .pass { color: var(--green); }
  .fail { color: var(--red); }
  .warn { color: var(--orange); }
  .pulse {
    display: inline-block;
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--green);
    margin-right: 6px;
    animation: pulse-anim 2s infinite;
  }
  @keyframes pulse-anim {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }
  .footer {
    text-align: center;
    color: var(--dim);
    font-size: 0.75rem;
    margin-top: 2rem;
  }
  #error-bar {
    display: none;
    background: rgba(255,68,102,0.15);
    border: 1px solid var(--red);
    border-radius: 8px;
    padding: 0.75rem 1rem;
    margin-bottom: 1.5rem;
    color: var(--red);
    font-size: 0.85rem;
  }
</style>
</head>
<body>
  <h1><span class="pulse"></span> Trytet Console</h1>
  <p class="subtitle">Control Plane — Live Northstar Metrics</p>
  <div id="error-bar"></div>
  <div class="grid" id="metrics-grid">
    <div class="card">
      <div class="card-label">Teleport Warp</div>
      <div class="card-value" id="v-warp">—<span class="card-unit">µs</span></div>
      <div class="card-status" id="s-warp">Waiting...</div>
    </div>
    <div class="card">
      <div class="card-label">Mitosis Constant</div>
      <div class="card-value" id="v-mitosis">—<span class="card-unit">µs</span></div>
      <div class="card-status" id="s-mitosis">Waiting...</div>
    </div>
    <div class="card">
      <div class="card-label">Oracle Fidelity</div>
      <div class="card-value" id="v-oracle">—<span class="card-unit">µs</span></div>
      <div class="card-status" id="s-oracle">Waiting...</div>
    </div>
    <div class="card">
      <div class="card-label">Market Evacuation</div>
      <div class="card-value" id="v-evac">—<span class="card-unit">ms</span></div>
      <div class="card-status" id="s-evac">Waiting...</div>
    </div>
    <div class="card">
      <div class="card-label">Fuel Efficiency</div>
      <div class="card-value" id="v-eff">—</div>
      <div class="card-status" id="s-eff">Waiting...</div>
    </div>
  </div>
  <div class="footer">Auto-refreshing every 2s · <code>GET /v1/swarm/metrics</code></div>
<script>
function check(val, ceil) {
  return val < ceil ? '<span class="pass">✔ PASS</span>' : '<span class="fail">✘ FAIL</span>';
}
async function poll() {
  try {
    const r = await fetch('/v1/swarm/metrics');
    if (!r.ok) throw new Error('HTTP ' + r.status);
    const d = await r.json();
    document.getElementById('error-bar').style.display = 'none';

    document.getElementById('v-warp').innerHTML = (d.teleport_warp_us||0).toLocaleString() + '<span class="card-unit">µs</span>';
    document.getElementById('s-warp').innerHTML = 'Ceiling: 200,000µs · ' + check(d.teleport_warp_us, 200000);

    document.getElementById('v-mitosis').innerHTML = (d.mitosis_latency_us||0).toLocaleString() + '<span class="card-unit">µs</span>';
    document.getElementById('s-mitosis').innerHTML = 'Ceiling: 15,000µs · ' + check(d.mitosis_latency_us, 15000);

    document.getElementById('v-oracle').innerHTML = (d.oracle_verification_us||0).toLocaleString() + '<span class="card-unit">µs</span>';
    document.getElementById('s-oracle').innerHTML = 'Ceiling: 5,000µs · ' + check(d.oracle_verification_us, 5000);

    document.getElementById('v-evac').innerHTML = (d.market_evacuation_ms||0).toLocaleString() + '<span class="card-unit">ms</span>';
    document.getElementById('s-evac').innerHTML = 'Ceiling: 800ms · ' + check(d.market_evacuation_ms, 800);

    document.getElementById('v-eff').innerHTML = (d.fuel_efficiency_ratio||0).toFixed(4);
    document.getElementById('s-eff').innerHTML = '<span class="pass">Higher = Better</span>';
  } catch(e) {
    const bar = document.getElementById('error-bar');
    bar.style.display = 'block';
    bar.textContent = 'Engine unreachable: ' + e.message;
  }
}
poll();
setInterval(poll, 2000);
</script>
</body>
</html>"#;

/// Returns an Axum sub-router that serves the Console dashboard.
///
/// Mount this at `/console` or as an independent listener on a separate port.
pub fn console_router() -> Router {
    Router::new()
        .route("/", get(serve_console_page))
        .route("/console", get(serve_console_page))
}

/// Serves the embedded Console dashboard page.
///
/// This is exported publicly so it can be mounted directly in a
/// stateful router via `.route("/console", get(serve_console_page))`.
pub async fn serve_console_page() -> impl IntoResponse {
    Html(CONSOLE_HTML)
}
