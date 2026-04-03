use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Terminal,
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn Error>> {
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
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), Box<dyn Error>> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    // In a fully integrated system, we would subscribe to `tokio::sync::broadcast` natively.
    // For the standalone TUI binary, we simulate the high-Hz feed reading from the OS layer.

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Percentage(50),
                        Constraint::Percentage(50),
                    ]
                    .as_ref(),
                )
                .split(f.area());

            let header = Paragraph::new("TRYTET SOVEREIGN STUDIO - HIVE PULSE TUI")
                .style(Style::default().fg(Color::Cyan))
                .block(Block::default().borders(Borders::ALL).title("Core"));
            f.render_widget(header, chunks[0]);

            let heartbeats = vec![
                Row::new(vec!["researcher_1", "Running", "12ms", "64MB", "0"]),
                Row::new(vec!["librarian_1", "Booting", "0ms", "0MB", "0"]),
                Row::new(vec!["analyst_1", "Running", "8ms", "12MB", "0"]),
            ];

            let heartbeat_table = Table::new(
                heartbeats,
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                    Constraint::Percentage(20),
                ],
            )
            .header(Row::new(vec![
                "Agent",
                "Status",
                "Latency",
                "Memory",
                "Vector Pressure",
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Swarm Heartbeat"),
            );

            f.render_widget(heartbeat_table, chunks[1]);

            let mesh_events = vec![
                Row::new(vec![
                    "12:00:01",
                    "researcher_1 -> librarian_1",
                    "Query HNSW Segment",
                ]),
                Row::new(vec![
                    "12:00:02",
                    "librarian_1 -> memory",
                    "Recall k=5 (0.5ms)",
                ]),
            ];

            let mesh_map = Table::new(
                mesh_events,
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(40),
                    Constraint::Percentage(40),
                ],
            )
            .header(Row::new(vec!["Timestamp", "Edge", "Payload"]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Mesh Telemetry Stream"),
            );

            f.render_widget(mesh_map, chunks[2]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    return Ok(());
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}
