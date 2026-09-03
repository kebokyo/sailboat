//! The work items pane: priority, number, state, name.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Cell, HighlightSpacing, Row, StatefulWidget, Table, Widget},
};
use ratatui::prelude::Stylize;

use crate::{api::types::WorkItem, app::widgets::{ListWorkItemsWidget, LoadingState}};

use super::{Focused, RowStyle, assignee_spans, display_text, label_spans, priority_span, state_spans};

/// Column width for a work item's state name. Fits Plane's default states
/// ("In Progress" is the longest at 11).
pub(crate) const STATE_WIDTH: usize = 12;

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
            Constraint::Length(META_WIDTH as u16),
        ];
        let table = Table::new(rows, widths)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}

/// Width of the trailing metadata column.
const META_WIDTH: usize = 44;

/// One row of the work items table: priority, number, state, name, metadata.
fn work_item_row(item: &WorkItem, dimmed: bool) -> Row<'static> {
    let style = RowStyle::new(dimmed);

    let state = item.state.as_ref().and_then(|state| state.expanded());

    Row::new(vec![
        Cell::from(priority_span(item.priority, dimmed)),
        Cell::from(Span::from(format!("#{}", item.sequence_id)).fg(if dimmed {
            Color::DarkGray
        } else {
            Color::Gray
        })),
        Cell::from(Line::from(state_spans(state, dimmed))),
        // Variation selectors stripped: an emoji at the end of a name would
        // otherwise be measured one cell and drawn two, dragging the row out of
        // alignment and cutting the highlight short.
        Cell::from(Span::from(display_text(&item.name)).fg(style.text)),
        // Right-aligned so the eye can scan down the column even though the
        // names to its left are ragged.
        Cell::from(Line::from(meta_spans(item, dimmed)).right_aligned()),
    ])
}

/// The trailing summary: who, what labels, and the dates that matter.
///
/// Ordered least- to most-specific so the rightmost thing is always a date,
/// which is what the column is mostly scanned for.
fn meta_spans(item: &WorkItem, dimmed: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut push = |mut group: Vec<Span<'static>>, spans: &mut Vec<Span<'static>>| {
        if group.is_empty() {
            return;
        }
        if !spans.is_empty() {
            spans.push(Span::from("  "));
        }
        spans.append(&mut group);
    };

    push(assignee_spans(&item.assignees, dimmed), &mut spans);
    push(label_spans(&item.labels, dimmed), &mut spans);

    if let Some(target) = item.target_date {
        let overdue = item.completed_at.is_none() && target < chrono::Utc::now().date_naive();
        let colour = match (dimmed, overdue) {
            (true, _) => Color::DarkGray,
            // A missed date is the one thing in this column worth interrupting for.
            (false, true) => Color::LightRed,
            (false, false) => Color::Yellow,
        };
        push(
            vec![Span::from(format!("⏱ {}", target.format("%m-%d"))).fg(colour)],
            &mut spans,
        );
    }

    if let Some(completed) = item.completed_at {
        let colour = if dimmed { Color::DarkGray } else { Color::LightGreen };
        push(
            vec![Span::from(format!("✔{}", completed.format("%m-%d"))).fg(colour)],
            &mut spans,
        );
    }

    spans
}
