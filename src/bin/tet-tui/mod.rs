//! Hive-Pulse TUI
//!
//! Real-time observability dashboard.
//! Provides real-time visualization of agent health, fuel consumption,
//! teleportation traces, Oracle cache density, and context pressure.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::{
    collections::HashMap,
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tet_core::telemetry::{HiveEvent, TelemetryHub};
use tokio::sync::broadcast;

mod renders;
mod events;
mod app;
use renders::*;
use events::*;
use app::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const EVENT_BUFFER_CAPACITY: usize = 1024;
const TICK_RATE_MS: u64 = 16; // ~60fps

// ---------------------------------------------------------------------------
// Agent State
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct AgentState {
    _tet_id: String,
    alias: String,
    status: String,
    fuel_consumed: u64,
    fuel_limit: u64,
    memory_kb: u64,
    last_updated_us: u64,
}

impl AgentState {
    fn fuel_pct(&self) -> f64 {
        if self.fuel_limit == 0 {
            return 0.0;
        }
        (self.fuel_consumed as f64 / self.fuel_limit as f64) * 100.0
    }

    fn fuel_pressure(&self) -> f64 {
        if self.fuel_limit == 0 {
            return 0.0;
        }
        self.fuel_consumed as f64 / self.fuel_limit as f64
    }
}

// ---------------------------------------------------------------------------
// Oracle Stats
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct OracleStats {
    cache_hits: u64,
    cache_misses: u64,
    total_inferences: u64,
    cached_inferences: u64,
}

impl OracleStats {
    fn hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 0.0;
        }
        self.cache_hits as f64 / total as f64
    }
}

// ---------------------------------------------------------------------------
// App State
// ---------------------------------------------------------------------------

struct AppState {
    agents: HashMap<String, AgentState>,
    event_log: Vec<String>,
    oracle: OracleStats,
    total_events: u64,
    uptime_start: Instant,
    selected_agent: usize,
    scroll_offset: usize,
}

impl AppState {
    fn new() -> Self {
        Self {
            agents: HashMap::new(),
            event_log: Vec::with_capacity(EVENT_BUFFER_CAPACITY),
            oracle: OracleStats::default(),
            total_events: 0,
            uptime_start: Instant::now(),
            selected_agent: 0,
            scroll_offset: 0,
        }
    }

    fn push_event_log(&mut self, msg: String) {
        if self.event_log.len() >= EVENT_BUFFER_CAPACITY {
            self.event_log.remove(0);
        }
        self.event_log.push(msg);
    }

}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_dashboard(f: &mut Frame, state: &AppState) {
    // Top-level 3-row layout: Header | Body | EventLog
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Body
            Constraint::Length(8), // Event Log
        ])
        .split(f.area());

    render_header(f, state, main[0]);
    render_body(f, state, main[1]);
    render_event_log(f, state, main[2]);
}


fn render_body(f: &mut Frame, state: &AppState, area: Rect) {
    // Split body: left 60% (Agent Table) | right 40% (Gauges)
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_agent_table(f, state, body[0]);
    render_gauges(f, state, body[1]);
}




// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Hive-Pulse TUI error: {:?}", err);
    }

    Ok(())
}

