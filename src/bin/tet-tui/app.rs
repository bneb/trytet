use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table},
    Frame,
};
use crate::models::TetExecutionResult;
use crate::market::HiveMarket;

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
    state.push_event_log("  Hive-Pulse TUI v16.1 — Swarm Observability".to_string());
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
