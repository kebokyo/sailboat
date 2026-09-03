use ratatui::{Frame, buffer::Buffer, layout::{Constraint, Direction, Layout, Offset, Rect}, style::{Color, Style}, symbols, text::{Line, Span, Text}, widgets::{Block, Borders, HighlightSpacing, List, ListItem, Paragraph, Row, StatefulWidget, Tabs, Widget}};
use ratatui::prelude::Stylize;

use crate::{api::types::{Priority, Project, WorkItem}, app::{App, ListProjectsWidget, ListWorkItemsWidget}};

pub fn ui(frame: &mut Frame, app: &App) {
    let top_level = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Line::from(Span::from("sailboat - v0.1.0 - millicent").fg(Color::LightMagenta));
    frame.render_widget(header, top_level[0] + Offset::new(1, 0));

    let main_block = Block::bordered();
    let content_area = main_block.inner(top_level[1]);
    frame.render_widget(main_block, top_level[1]);

    let tabs = Tabs::new(vec![app.current_workspace.clone()])
        .style(Color::White)
        .highlight_style(Style::default().magenta().on_black().bold())
        .select(0)
        .divider(symbols::DOT)
        .padding(" ", " ");
    frame.render_widget(tabs, top_level[1] + Offset::new(1, 0));

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Fill(1)
        ])
        .split(content_area);

    frame.render_widget(&app.list_projects, content_layout[0]);
    frame.render_widget(&app.list_work_items, content_layout[1]);

    let footer = Line::from(Span::from("j/k or ↑/↓ to scroll | enter to open a project | q to quit").fg(Color::White));
    frame.render_widget(footer, top_level[2] + Offset::new(1, 0));
}

impl Widget for &ListProjectsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = self.state.write().unwrap();

        // a block with a right aligned title with the loading state on the right
        let loading_state = Line::from(format!("{:?}", state.loading_state)).right_aligned();
        let block = Block::new()
            .borders(Borders::RIGHT)
            .title("projects")
            .title(loading_state);

        // a table with the list of pull requests
        let items = state.projects.iter();
        let list = List::new(items)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(list, area, buf, &mut state.list_state);
    }
}

impl Widget for &ListWorkItemsWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = self.state.write().unwrap();

        let loading_state = Line::from(format!("{:?}", state.loading_state)).right_aligned();
        let block = Block::new()
            .title("work items")
            .title(loading_state);

        let items = state.work_items.iter();
        let list = List::new(items)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(list, area, buf, &mut state.list_state);
    }
}

impl From<&Project> for ListItem<'_> {
    fn from(proj: &Project) -> Self {
        let p = proj.clone();
        let spans = vec![
            Span::raw(p.identifier),
            Span::raw(" - "),
            Span::raw(p.name),
        ];

        Line::from(spans).into()
    }
}

/// Column width for a work item's state name. Fits Plane's default states
/// ("In Progress" is the longest at 11); anything longer is truncated.
const STATE_WIDTH: usize = 12;

impl From<&WorkItem> for ListItem<'_> {
    fn from(item: &WorkItem) -> Self {
        // A glyph carries priority without spending a column on the word.
        let priority = match item.priority {
            Priority::Urgent => Span::from("! ").fg(Color::LightRed),
            Priority::High => Span::from("^ ").fg(Color::LightYellow),
            Priority::Medium => Span::from("- ").fg(Color::White),
            Priority::Low => Span::from("v ").fg(Color::LightBlue),
            Priority::None => Span::from("  "),
        };

        // Present only because the request asked for `expand=state`; without it
        // this field is a bare uuid and there is nothing worth showing. Padded to
        // a fixed width either way, so the names to its right stay in a column
        // instead of stepping in and out with each state's length.
        let state = item
            .state
            .as_ref()
            .and_then(|state| state.expanded())
            .map(|state| state.name.as_str())
            .unwrap_or("");

        let spans = vec![
            priority,
            Span::from(format!("#{:<4} ", item.sequence_id)).fg(Color::DarkGray),
            Span::from(format!("{state:<STATE_WIDTH$.STATE_WIDTH$} ")).fg(Color::Cyan),
            Span::from(item.name.clone()),
        ];

        Line::from(spans).into()
    }
}
