use std::io;

use crossterm::{execute, terminal};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType},
};

use crate::metrics::History;

pub struct AppState {
    pub history: History,
}

impl AppState {
    pub fn new(capacity: usize) -> Self {
        Self {
            history: History::with_capacity(capacity),
        }
    }
}

pub fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal() -> anyhow::Result<()> {
    terminal::disable_raw_mode()?;
    execute!(
        io::stdout(),
        terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    Ok(())
}

pub fn draw_ui(frame: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(frame.area());

    // Title bar / status
    let title = Block::default().title("rasitop").borders(Borders::ALL);
    frame.render_widget(title, chunks[0]);

    let top = chunks[1];
    let bottom = chunks[2];
    let top_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top);

    render_chart_cpu_power(frame, top_split[0], app);
    render_chart_busy(frame, top_split[1], app);
    render_chart_busy_p(frame, bottom, app);
}

fn history_to_points_cpu_power(app: &AppState) -> Vec<(f64, f64)> {
    app.history
        .iter()
        .enumerate()
        .map(|(idx, s)| (idx as f64, s.cpu_power_mw / 1000.0))
        .collect()
}

fn history_to_points_busy<F>(app: &AppState, f: F) -> Vec<(f64, f64)>
where
    F: Fn(&crate::pm::PowermetricsSample) -> Option<f64>,
{
    app.history
        .iter()
        .enumerate()
        .map(|(idx, s)| (idx as f64, f(s).unwrap_or(0.0)))
        .collect()
}

fn render_chart_cpu_power(frame: &mut Frame, area: Rect, app: &AppState) {
    let data = history_to_points_cpu_power(app);
    let dataset = Dataset::default()
        .name("CPU power (W)")
        .marker(symbols::Marker::Dot)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Yellow))
        .data(&data);
    let chart = Chart::new(vec![dataset])
        .block(Block::default().title("CPU Power").borders(Borders::ALL))
        .x_axis(Axis::default().title("t").bounds(x_bounds(data.len())))
        .y_axis(Axis::default().title("W").bounds(auto_y_bounds(&data, 2.0)));
    frame.render_widget(chart, area);
}

fn render_chart_busy(frame: &mut Frame, area: Rect, app: &AppState) {
    let data = history_to_points_busy(app, |s| s.e_busy_ratio);
    let dataset = Dataset::default()
        .name("E busy")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&data);
    let chart = Chart::new(vec![dataset])
        .block(Block::default().title("E Busy").borders(Borders::ALL))
        .x_axis(Axis::default().title("t").bounds(x_bounds(data.len())))
        .y_axis(Axis::default().title("ratio").bounds([0.0, 1.0]));
    frame.render_widget(chart, area);
}

fn render_chart_busy_p(frame: &mut Frame, area: Rect, app: &AppState) {
    let data = history_to_points_busy(app, |s| s.p_busy_ratio);
    let dataset = Dataset::default()
        .name("P busy")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Green))
        .data(&data);
    let chart = Chart::new(vec![dataset])
        .block(Block::default().title("P Busy").borders(Borders::ALL))
        .x_axis(Axis::default().title("t").bounds(x_bounds(data.len())))
        .y_axis(Axis::default().title("ratio").bounds([0.0, 1.0]));
    frame.render_widget(chart, area);
}

fn x_bounds(len: usize) -> [f64; 2] {
    let n = len as f64;
    if n <= 1.0 { [0.0, 1.0] } else { [n - 100.0, n] }
}

fn auto_y_bounds(data: &[(f64, f64)], pad: f64) -> [f64; 2] {
    let max_y = data.iter().map(|(_, y)| *y).fold(0.0, f64::max);
    [0.0, (max_y + pad).max(100.0)]
}
