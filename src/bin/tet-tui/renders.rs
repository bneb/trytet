use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table},
    Frame,
};
use tet_core::models::TetExecutionResult;
use tet_core::market::HiveMarket;
use super::AppState;

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
                " Trytet Studio ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(header, area);
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
