use super::feed::{FeedApp, FeedPanel};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::DefaultTerminal;
use std::io;
use std::time::Duration;

/// Run the interactive TUI feed.
pub fn run_tui(problems_mode: bool) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = FeedApp::new(1000);
    if problems_mode {
        app.toggle_problems();
    }
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, app: &mut FeedApp) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        // Poll for events with a timeout (allows future async event injection)
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_up(),
                    KeyCode::Tab => app.next_panel(),
                    KeyCode::Char('1') => app.jump_to_panel(1),
                    KeyCode::Char('2') => app.jump_to_panel(2),
                    KeyCode::Char('3') => app.jump_to_panel(3),
                    KeyCode::Char('p') => app.toggle_problems(),
                    _ => {}
                }
            }
        }
    }
}

fn draw(frame: &mut ratatui::Frame, app: &FeedApp) {
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(50),
        ])
        .split(outer_chunks[0]);

    // Agents panel
    let agents_block = panel_block("Agents [1]", app.active_panel == FeedPanel::Agents);
    let agents_content = Paragraph::new("No active agents")
        .style(Style::default().fg(Color::DarkGray))
        .block(agents_block);
    frame.render_widget(agents_content, main_chunks[0]);

    // Convoys panel
    let convoys_block = panel_block("Convoys [2]", app.active_panel == FeedPanel::Convoys);
    let convoys_content = Paragraph::new("No active convoys")
        .style(Style::default().fg(Color::DarkGray))
        .block(convoys_block);
    frame.render_widget(convoys_content, main_chunks[1]);

    // Events panel
    let events_block = panel_block(
        if app.problems_mode {
            "Events [3] (problems only)"
        } else {
            "Events [3]"
        },
        app.active_panel == FeedPanel::Events,
    );

    let visible = app.visible_events();
    if visible.is_empty() {
        let empty = Paragraph::new("No events to display. Waiting for activity...")
            .style(Style::default().fg(Color::DarkGray))
            .block(events_block);
        frame.render_widget(empty, main_chunks[2]);
    } else {
        let items: Vec<ListItem> = visible
            .iter()
            .skip(app.events_scroll)
            .map(|event| {
                let symbol = event.event_type.symbol();
                let line = Line::from(vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("[{}] ", event.role),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        format!("{}: ", event.agent),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&event.message),
                ]);
                ListItem::new(line)
            })
            .collect();
        let events_list = List::new(items).block(events_block);
        frame.render_widget(events_list, main_chunks[2]);
    }

    // Status bar
    let mode = if app.problems_mode { "PROBLEMS" } else { "ALL" };
    let status = Line::from(vec![
        Span::styled(
            " Augusta Feed ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" Mode: {mode} ")),
        Span::styled(
            " j/k:scroll  Tab:panel  1/2/3:jump  p:problems  q:quit ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let status_bar = Paragraph::new(status);
    frame.render_widget(status_bar, outer_chunks[1]);
}

fn panel_block(title: &str, active: bool) -> Block<'_> {
    let style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(style)
}
