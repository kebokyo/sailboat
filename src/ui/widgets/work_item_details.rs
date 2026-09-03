// CDDL HEADER START
//
// The contents of this file are subject to the terms of the
// Common Development and Distribution License (the "License").
// You may not use this file except in compliance with the License.
//
// You can obtain a copy of the license in the file LICENSE
// or at https://opensource.org/licenses/CDDL-1.0.
// See the License for the specific language governing permissions
// and limitations under the License.
//
// When distributing Covered Code, include this CDDL HEADER in each
// file and include the License file. If applicable, add the following
// below this CDDL HEADER, with the fields enclosed by brackets "[]"
// replaced with your own identifying information:
// Portions Copyright [yyyy] [name of copyright owner]
//
// CDDL HEADER END
//
// Copyright 2026 millie.moe. All rights reserved.
// Use is subject to license terms.
//! The details pane: one work item's attributes, one per row.

use chrono::{DateTime, NaiveDate, Utc};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, HighlightSpacing, Row, StatefulWidget, Table, Widget},
};
use ratatui::prelude::Stylize;

use crate::{api::types::WorkItem, app::widgets::{LoadingState, WorkItemDetailsWidget}};

use super::{Focused, RowStyle, assignee_spans, label_spans, priority_span, priority_span_long, state_spans};

impl Widget for Focused<'_, WorkItemDetailsWidget> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Focused(widget, focused) = self;
        let mut state = widget.state.write().unwrap();

        // Same rule as the list panes: focus alone is not enough.
        let ready = state.loading_state == LoadingState::Loaded;
        let dimmed = !(focused && ready);
        let title = if dimmed { Color::DarkGray } else { Color::White };
        let loading = Line::from(format!("{:?}", state.loading_state)).right_aligned();
        let block = Block::new()
            .borders(Borders::RIGHT)
            .title(Span::from("details").fg(title))
            .title(loading);

        // Unlike the list panes, every row here is a different attribute rather
        // than another instance of one thing, so the table is label + value.
        let rows: Vec<Row> = match &state.work_item {
            Some(item) => attribute_rows(item, dimmed),
            None => Vec::new(),
        };

        let widths = [
            Constraint::Length(ATTRIBUTE_LABEL_WIDTH as u16),
            Constraint::Fill(1),
        ];
        let table = Table::new(rows, widths)
            .block(block)
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");

        StatefulWidget::render(table, area, buf, &mut state.table_state);
    }
}

/// Width of the attribute-name column in the details table.
const ATTRIBUTE_LABEL_WIDTH: usize = 12;

/// The attributes of a work item, one per row.
///
/// Rows are emitted unconditionally, including for attributes that are unset.
/// A details table that hides empty rows renumbers itself as data changes, and
/// since the selected row is what an edit dialog would open on, a stable set of
/// positions matters more than a compact one. Unset values show a placeholder.
///
/// `description_html` is deliberately absent. It is prose, often many lines and
/// needing its own rendering, so it belongs in a region of its own rather than
/// squeezed into a table cell.
/// A list of spans, or the placeholder when there are none.
fn or_unset(spans: Vec<Span<'static>>, placeholder: Span<'static>) -> Line<'static> {
    if spans.is_empty() {
        Line::from(placeholder)
    } else {
        Line::from(spans)
    }
}

fn attribute_rows(item: &WorkItem, dimmed: bool) -> Vec<Row<'static>> {
    let style = RowStyle::new(dimmed);

    let unset = || Span::from("--").fg(Color::DarkGray);
    let value = |text: String| Span::from(text).fg(style.text);

    // Expanded relations render as names; unexpanded ones only have an id worth
    // showing as a placeholder, since a raw uuid tells the reader nothing.
    let named = |expanded: Option<String>| match expanded {
        Some(name) => Span::from(name).fg(style.key),
        None => unset(),
    };


    let date = |date: &Option<NaiveDate>| match date {
        Some(date) => value(date.format("%Y-%m-%d").to_string()),
        None => unset(),
    };

    let stamp = |at: &Option<DateTime<Utc>>| match at {
        Some(at) => value(at.format("%Y-%m-%d %H:%M").to_string()),
        None => unset(),
    };

    let attributes: Vec<(&str, Line<'static>)> = vec![
        ("key", Line::from(value(format!("#{}", item.sequence_id)))),
        (
            "state",
            // Icon and colour by group, matching the work items list.
            match item.state.as_ref().and_then(|state| state.expanded()) {
                Some(state) => Line::from(state_spans(Some(state), dimmed)),
                None => Line::from(unset()),
            },
        ),
        (
            "priority",
            // Same glyph and colour as the list, so one fact shown in two places
            // cannot drift apart.
            Line::from(priority_span_long(item.priority, dimmed)),
        ),
        ("assignees", or_unset(assignee_spans(&item.assignees, dimmed), unset())),
        // Each label in its own colour rather than a bare count.
        ("labels", or_unset(label_spans(&item.labels, dimmed), unset())),
        ("start", Line::from(date(&item.start_date))),
        ("target", Line::from(date(&item.target_date))),
        (
            "estimate",
            Line::from(named(item.estimate_point.as_ref().and_then(|point| {
                point.expanded().map(|point| point.value.clone())
            }))),
        ),
        (
            "parent",
            Line::from(named(item.parent.as_ref().and_then(|parent| {
                parent.expanded().map(|parent| format!("#{}", parent.sequence_id))
            }))),
        ),
        ("completed", Line::from(stamp(&item.completed_at))),
        ("created", Line::from(stamp(&item.created_at))),
        ("updated", Line::from(stamp(&item.updated_at))),
    ];

    attributes
        .into_iter()
        .map(|(label, rendered)| {
            Row::new(vec![
                Cell::from(Span::from(label).fg(style.accent)),
                Cell::from(rendered),
            ])
        })
        .collect()
}
