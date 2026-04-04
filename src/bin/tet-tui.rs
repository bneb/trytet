//! Hive-Pulse TUI — Phase 16.1
//!
//! The Sovereign Swarm observability dashboard.
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

    fn process_event(&mut self, event: HiveEvent) {
        self.total_events += 1;

        match event {
            HiveEvent::AgentBooted {
                tet_id,
                alias,
                fuel_limit,
                memory_limit_mb,
                timestamp_us,
            } => {
                let display_alias = alias.clone().unwrap_or_else(|| tet_id[..8].to_string());
                self.agents.insert(
                    tet_id.clone(),
                    AgentState {
                        _tet_id: tet_id.clone(),
                        alias: display_alias.clone(),
                        status: "Running".to_string(),
                        fuel_consumed: 0,
                        fuel_limit,
                        memory_kb: memory_limit_mb as u64 * 1024,
                        last_updated_us: timestamp_us,
                    },
                );
                self.push_event_log(format!(
                    "▶  BOOT  {} (fuel: {}, mem: {}MB)",
                    display_alias, fuel_limit, memory_limit_mb
                ));
            }

            HiveEvent::AgentCompleted {
                tet_id,
                status,
                fuel_consumed,
                fuel_limit,
                memory_used_kb,
                duration_us,
                timestamp_us,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&tet_id) {
                    agent.status = status.clone();
                    agent.fuel_consumed = fuel_consumed;
                    agent.memory_kb = memory_used_kb;
                    agent.last_updated_us = timestamp_us;
                }
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| tet_id[..8].to_string());
                self.push_event_log(format!(
                    "■  DONE  {} → {} ({:.1}ms, {}/{} fuel)",
                    alias,
                    status,
                    duration_us as f64 / 1000.0,
                    fuel_consumed,
                    fuel_limit
                ));
            }

            HiveEvent::FuelConsumed {
                tet_id,
                operation,
                amount,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&tet_id) {
                    agent.fuel_consumed += amount;
                }
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| tet_id[..8].to_string());
                self.push_event_log(format!("⚡ FUEL  {} -{} ({})", alias, amount, operation));
            }

            HiveEvent::TeleportInitiated {
                agent_id,
                target_node,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.status = "MIGRATING".to_string();
                }
                self.push_event_log(format!("✈  TELE  {} → {}", agent_id, target_node));
            }

            HiveEvent::TeleportCompleted {
                agent_id,
                target_node,
                bytes_transferred,
                ..
            } => {
                self.push_event_log(format!(
                    "✓  LAND  {} @ {} ({:.1}KB)",
                    agent_id,
                    target_node,
                    bytes_transferred as f64 / 1024.0
                ));
            }

            HiveEvent::OracleHit {
                tet_id,
                request_hash,
                ..
            } => {
                self.oracle.cache_hits += 1;
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!("◉  HIT   {} hash:{}…", alias, &request_hash[..12]));
            }

            HiveEvent::OracleMiss { tet_id, url, .. } => {
                self.oracle.cache_misses += 1;
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!("○  MISS  {} → {}", alias, url));
            }

            HiveEvent::InferenceStarted {
                tet_id,
                model_id,
                prompt_tokens_est,
                ..
            } => {
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "guest".to_string());
                self.push_event_log(format!(
                    "🧠 INFER {} → {} (~{}tok)",
                    alias, model_id, prompt_tokens_est
                ));
            }

            HiveEvent::InferenceCompleted {
                model_id,
                input_tokens,
                output_tokens,
                fuel_cost,
                cached,
                ..
            } => {
                self.oracle.total_inferences += 1;
                if cached {
                    self.oracle.cached_inferences += 1;
                }
                let tag = if cached { "CACHED" } else { "FRESH" };
                self.push_event_log(format!(
                    "🧠 {}  {} in:{}→out:{} (fuel:{})",
                    tag, model_id, input_tokens, output_tokens, fuel_cost
                ));
            }

            HiveEvent::ContextPruned {
                tet_id,
                tokens_removed,
                blocks_evicted,
                ..
            } => {
                let alias = self
                    .agents
                    .get(&tet_id)
                    .map(|a| a.alias.clone())
                    .unwrap_or_else(|| "?".to_string());
                self.push_event_log(format!(
                    "✂  PRUNE {} -{} tokens ({} blocks)",
                    alias, tokens_removed, blocks_evicted
                ));
            }
        }
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

fn render_header(f: &mut Frame, state: &AppState, area: Rect) {
    let uptime = state.uptime_start.elapsed();
    let mins = uptime.as_secs() / 60;
    let secs = uptime.as_secs() % 60;

    let avg_fuel: f64 = if state.agents.is_empty() {
        0.0
    } else {
        state
            .agents
            .values()
            .map(|a| a.fuel_pressure())
            .sum::<f64>()
            / state.agents.len() as f64
    };

    let header_text = vec![Line::from(vec![
        Span::styled(
            " HIVE-PULSE ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("AGENTS: {} ", state.agents.len()),
            Style::default().fg(Color::Green),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("EVENTS: {} ", state.total_events),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("FUEL PRESSURE: {:.0}% ", avg_fuel * 100.0),
            Style::default().fg(if avg_fuel > 0.8 {
                Color::Red
            } else if avg_fuel > 0.5 {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
        Span::raw("│ "),
        Span::styled(
            format!("UP: {:02}:{:02}", mins, secs),
            Style::default().fg(Color::DarkGray),
        ),
    ])];

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                " Trytet Sovereign Studio ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(header, area);
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

fn render_agent_table(f: &mut Frame, state: &AppState, area: Rect) {
    let mut agents: Vec<&AgentState> = state.agents.values().collect();
    agents.sort_by(|a, b| a.alias.cmp(&b.alias));

    let rows: Vec<Row> = agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let _status_color = match a.status.as_str() {
                "Running" => Color::Green,
                "Success" => Color::Cyan,
                "MIGRATING" => Color::Magenta,
                "OutOfFuel" => Color::Red,
                _ if a.status.starts_with("Crash") => Color::Red,
                _ => Color::Gray,
            };

            let fuel_bar = if a.fuel_limit > 0 {
                let pct = a.fuel_pct().min(100.0);
                let blocks = (pct / 5.0) as usize;
                let bar: String = "█".repeat(blocks) + &"░".repeat(20 - blocks.min(20));
                format!("{:.0}% {}", pct, bar)
            } else {
                "N/A".to_string()
            };

            let style = if i == state.selected_agent {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                a.alias.clone(),
                a.status.clone(),
                format!("{}KB", a.memory_kb),
                fuel_bar,
            ])
            .style(style)
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(45),
        ],
    )
    .header(
        Row::new(vec!["Agent", "Status", "Memory", "Fuel Pressure"])
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                " Swarm Overview ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(table, area);
}

fn render_gauges(f: &mut Frame, state: &AppState, area: Rect) {
    let gauge_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(33), // Fuel
            Constraint::Percentage(34), // Oracle
            Constraint::Percentage(33), // Context
        ])
        .split(area);

    // -- Fuel Gauge --
    let avg_fuel = if state.agents.is_empty() {
        0.0
    } else {
        state
            .agents
            .values()
            .map(|a| a.fuel_pressure())
            .sum::<f64>()
            / state.agents.len() as f64
    };

    let fuel_color = if avg_fuel > 0.8 {
        Color::Red
    } else if avg_fuel > 0.5 {
        Color::Yellow
    } else {
        Color::Green
    };

    let fuel_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Fuel Gauge ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(fuel_color).bg(Color::DarkGray))
        .ratio(avg_fuel.min(1.0))
        .label(format!("{:.1}% avg pressure", avg_fuel * 100.0));
    f.render_widget(fuel_gauge, gauge_layout[0]);

    // -- Oracle Health --
    let hit_ratio = state.oracle.hit_ratio();
    let oracle_color = if hit_ratio > 0.7 {
        Color::Green
    } else if hit_ratio > 0.3 {
        Color::Yellow
    } else {
        Color::Red
    };

    let total_network = state.oracle.cache_hits + state.oracle.cache_misses;
    let oracle_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Oracle Health ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(oracle_color).bg(Color::DarkGray))
        .ratio(if total_network > 0 { hit_ratio } else { 0.0 })
        .label(format!(
            "Hit: {} Miss: {} ({:.0}%) | Thoughts: {}/{}",
            state.oracle.cache_hits,
            state.oracle.cache_misses,
            hit_ratio * 100.0,
            state.oracle.cached_inferences,
            state.oracle.total_inferences,
        ));
    f.render_widget(oracle_gauge, gauge_layout[1]);

    // -- Context Pressure --
    let inference_ratio = if state.oracle.total_inferences > 0 {
        state.oracle.cached_inferences as f64 / state.oracle.total_inferences as f64
    } else {
        0.0
    };

    let ctx_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Context Pressure ",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(Color::Blue).bg(Color::DarkGray))
        .ratio(inference_ratio.min(1.0))
        .label(format!("Cache efficiency: {:.0}%", inference_ratio * 100.0));
    f.render_widget(ctx_gauge, gauge_layout[2]);
}

fn render_event_log(f: &mut Frame, state: &AppState, area: Rect) {
    let visible_height = area.height.saturating_sub(2) as usize; // minus borders
    let start = state
        .event_log
        .len()
        .saturating_sub(visible_height + state.scroll_offset);
    let end = state.event_log.len().saturating_sub(state.scroll_offset);

    let lines: Vec<Line> = state.event_log[start..end]
        .iter()
        .map(|l| {
            let color = if l.starts_with("▶") {
                Color::Green
            } else if l.starts_with("■") {
                Color::Cyan
            } else if l.starts_with("✈") || l.starts_with("✓") {
                Color::Magenta
            } else if l.starts_with("◉") {
                Color::Green
            } else if l.starts_with("○") {
                Color::Red
            } else if l.starts_with("🧠") {
                Color::Yellow
            } else if l.starts_with("✂") {
                Color::Blue
            } else if l.starts_with("⚡") {
                Color::DarkGray
            } else {
                Color::Gray
            };
            Line::from(Span::styled(l.as_str(), Style::default().fg(color)))
        })
        .collect();

    let log = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                " Event Stream ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(log, area);
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

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(TICK_RATE_MS);
    let mut last_tick = Instant::now();

    // Create telemetry hub and subscribe
    let hub = Arc::new(TelemetryHub::default_capacity());
    let mut rx = hub.subscribe();

    let mut state = AppState::new();

    // In standalone mode, the TUI is a passive dashboard.
    // Events arrive from the broadcast channel when the engine is integrated.
    // For now, show an empty dashboard waiting for connections.
    state.push_event_log("  Hive-Pulse TUI v16.1 — Sovereign Swarm Observability".to_string());
    state.push_event_log("  Waiting for agent events... (press 'q' to quit)".to_string());

    loop {
        terminal.draw(|f| render_dashboard(f, &state))?;

        // Drain all pending events from the broadcast channel (non-blocking)
        loop {
            match rx.try_recv() {
                Ok(event) => state.process_event(event),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    state.push_event_log(format!("⚠ LAGGED — dropped {} events", n));
                    break;
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
                    KeyCode::Up => {
                        if state.selected_agent > 0 {
                            state.selected_agent -= 1;
                        }
                    }
                    KeyCode::Down => {
                        let max = state.agents.len().saturating_sub(1);
                        if state.selected_agent < max {
                            state.selected_agent += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        state.scroll_offset = state.scroll_offset.saturating_add(5);
                    }
                    KeyCode::PageDown => {
                        state.scroll_offset = state.scroll_offset.saturating_sub(5);
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}
