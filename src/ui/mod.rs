use ratatui::{Frame, buffer::Buffer, layout::{Constraint, Direction, Layout, Offset, Rect}, style::{Color, Style}, symbols, text::{Line, Span}, widgets::{Block, Borders, Cell, HighlightSpacing, Row, StatefulWidget, Table, Tabs, Widget}};
use ratatui::prelude::Stylize;

use crate::{api::types::{Priority, Project, WorkItem}, app::{App, CurrentScreen, ListProjectsWidget, ListWorkItemsWidget, LoadingState}};

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

    // Exactly one pane is live at a time; the other renders dimmed to show that
    // what it holds was fetched earlier and has not been refreshed since.
    let projects_focused = matches!(app.current_screen, CurrentScreen::MainProjectsView);
    let work_items_focused = matches!(app.current_screen, CurrentScreen::MainWorkItemsView);

    frame.render_widget(Focused(&app.list_projects, projects_focused), content_layout[0]);
    frame.render_widget(Focused(&app.list_work_items, work_items_focused), content_layout[1]);

    let footer = Line::from(Span::from("j/k or ↑/↓ to scroll | enter to open a project | q to quit").fg(Color::White));
    frame.render_widget(footer, top_level[2] + Offset::new(1, 0));
}

/// Pairs a list widget with whether its pane currently has focus.
///
/// Unfocused panes render dimmed. The colour is the signal that a pane holds
/// data the user is driving right now; grey means "this is what the server said
/// a while ago", and it stays grey until that pane is focused and its refresh
/// lands.
pub struct Focused<'a, T>(pub &'a T, pub bool);

/// Colour of a row that is current and in the focused pane.
struct RowStyle {
    key: Color,
    text: Color,
    accent: Color,
}

impl RowStyle {
    fn new(dimmed: bool) -> Self {
        if dimmed {
            // A single flat grey, so nothing in a stale pane draws the eye.
            RowStyle { key: Color::DarkGray, text: Color::DarkGray, accent: Color::DarkGray }
        } else {
            RowStyle { key: Color::Cyan, text: Color::White, accent: Color::LightMagenta }
        }
    }
}

impl Widget for Focused<'_, ListProjectsWidget> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Focused(widget, focused) = self;
        let mut state = widget.state.write().unwrap();

        let selected = state.table_state.selected();
        // Colour returns only once the refresh has landed, not the moment the
        // pane regains focus -- so a grey pane and ignored keypresses always
        // have the same visible cause.
        let ready = state.loading_state == LoadingState::Loaded;
        let title = if focused && ready { Color::White } else { Color::DarkGray };
        let loading = Line::from(format!("{:?}", state.loading_state)).right_aligned();
        let block = Block::new()
            .borders(Borders::RIGHT)
            .title(Span::from("projects").fg(title))
            .title(loading);

        let rows: Vec<Row> = state
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| {
                let dimmed = match (focused, ready) {
                    // Focused and settled: the pane the user is driving.
                    (true, true) => false,
                    // Focused but still refreshing. Everything greys, which is
                    // the same signal as input being ignored.
                    (true, false) => true,
                    // Unfocused: the selected project keeps its colour, since it
                    // is the one still driving the pane to its right.
                    (false, _) => Some(index) != selected,
                };
                let count = state.work_item_counts.get(&project.id).copied();
                project_row(project, count, dimmed)
            })
            .collect();

        let widths = [
            Constraint::Length(PROJECT_IDENTIFIER_WIDTH as u16),
            Constraint::Fill(1),
            Constraint::Length(4),
        ];
        let table = Table::new(rows, widths)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}

impl Widget for Focused<'_, ListWorkItemsWidget> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Focused(widget, focused) = self;
        let mut state = widget.state.write().unwrap();

        // Same rule as the projects pane: focus alone is not enough.
        let ready = state.loading_state == LoadingState::Loaded;
        let dimmed = !(focused && ready);
        let title = if dimmed { Color::DarkGray } else { Color::White };
        let loading = Line::from(format!("{:?}", state.loading_state)).right_aligned();
        let block = Block::new()
            .title(Span::from("work items").fg(title))
            .title(loading);

        let rows: Vec<Row> = state
            .work_items
            .iter()
            .map(|item| work_item_row(item, dimmed))
            .collect();

        let widths = [
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(STATE_WIDTH as u16),
            Constraint::Fill(1),
        ];
        let table = Table::new(rows, widths)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}

/// Width of the project identifier column. Plane caps identifiers at 12.
const PROJECT_IDENTIFIER_WIDTH: usize = 12;

/// Column width for a work item's state name. Fits Plane's default states
/// ("In Progress" is the longest at 11).
const STATE_WIDTH: usize = 12;

/// One row of the projects table: identifier, name, work item count.
fn project_row(project: &Project, work_items: Option<i64>, dimmed: bool) -> Row<'static> {
    let style = RowStyle::new(dimmed);

    // The count arrives on a separate request per project, so it lags the list.
    let count = match work_items {
        Some(count) => count.to_string(),
        None => "-".to_string(),
    };

    Row::new(vec![
        Cell::from(Span::from(project.identifier.clone()).fg(style.key)),
        Cell::from(Span::from(project.name.clone()).fg(style.text)),
        Cell::from(Line::from(Span::from(count).fg(style.accent)).right_aligned()),
    ])
}

/// One row of the work items table: priority, number, state, name.
fn work_item_row(item: &WorkItem, dimmed: bool) -> Row<'static> {
    let style = RowStyle::new(dimmed);

    // A glyph carries priority without spending a column on the word.
    let (glyph, colour) = match item.priority {
        Priority::Urgent => ("! ", Color::LightRed),
        Priority::High => ("^ ", Color::LightYellow),
        Priority::Medium => ("- ", Color::White),
        Priority::Low => ("v ", Color::LightBlue),
        Priority::None => ("  ", Color::White),
    };
    let priority = Span::from(glyph).fg(if dimmed { Color::DarkGray } else { colour });

    // Present only because the request asked for `expand=state`; without it this
    // field is a bare uuid and there is nothing worth showing. The column keeps
    // its width either way, so names stay aligned.
    let state = item
        .state
        .as_ref()
        .and_then(|state| state.expanded())
        .map(|state| state.name.to_string())
        .unwrap_or_default();

    let number = if dimmed { Color::DarkGray } else { Color::Gray };

    Row::new(vec![
        Cell::from(priority),
        Cell::from(Span::from(format!("#{}", item.sequence_id)).fg(number)),
        Cell::from(Span::from(state).fg(style.key)),
        Cell::from(Span::from(item.name.clone()).fg(style.text)),
    ])
}
