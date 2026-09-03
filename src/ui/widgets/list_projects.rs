//! The projects pane: identifier, name, and work item count.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, HighlightSpacing, Row, StatefulWidget, Table, Widget},
};
use ratatui::prelude::Stylize;

use crate::{api::types::Project, app::widgets::{ListProjectsWidget, LoadingState}};

use super::{Focused, RowStyle, display_text};

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

/// Width of the project identifier column. Plane caps identifiers at 12.
const PROJECT_IDENTIFIER_WIDTH: usize = 12;


/// One row of the projects table: identifier, name, work item count.
fn project_row(project: &Project, work_items: Option<i64>, dimmed: bool) -> Row<'static> {
    let style = RowStyle::new(dimmed);

    // The count arrives on a separate request per project, so it lags the list.
    let count = match work_items {
        Some(count) => count.to_string(),
        None => "-".to_string(),
    };

    Row::new(vec![
        Cell::from(Span::from(display_text(&project.identifier)).fg(style.key)),
        Cell::from(Span::from(display_text(&project.name)).fg(style.text)),
        Cell::from(Line::from(Span::from(count).fg(style.accent)).right_aligned()),
    ])
}
