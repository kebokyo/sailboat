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
//! The small dialog drawn over a pane, for editing one attribute of a work
//! item or naming a new one.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::app::{
    create::NewWorkItemDialog,
    edit::{Choice, EditDialog, Input},
};

use super::{priority_span_long, state_spans};

/// Widest the dialog gets, so a long name doesn't stretch it across the screen.
const MAX_WIDTH: u16 = 60;

pub struct DialogView<'a> {
    pub title: &'a str,
    pub input: &'a Input,
    /// Shown in red under the input.
    pub error: Option<&'a str>,
}

impl<'a> DialogView<'a> {
    pub fn edit(dialog: &'a EditDialog) -> Self {
        DialogView {
            title: dialog.attribute.label(),
            input: &dialog.input,
            error: dialog.error.as_deref(),
        }
    }

    pub fn new_work_item(dialog: &'a NewWorkItemDialog) -> Self {
        DialogView {
            title: "new work item",
            input: &dialog.input,
            error: dialog.error.as_deref(),
        }
    }
}

impl Widget for DialogView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let dialog = self;

        let body_height = match dialog.input {
            Input::Choice { options, .. } => options.len() as u16,
            Input::Text { .. } => 1,
        };
        let error_height = u16::from(dialog.error.is_some());
        // Two for the border.
        let popup = centred(area, MAX_WIDTH, body_height + error_height + 2);

        let block = Block::bordered()
            .border_style(Style::default().fg(Color::LightMagenta))
            .title(Span::from(format!(" {} ", dialog.title)).fg(Color::White));
        let inner = block.inner(popup);
        Clear.render(popup, buf);
        block.render(popup, buf);

        let body = Rect {
            height: inner.height.saturating_sub(error_height),
            ..inner
        };
        match dialog.input {
            Input::Choice { options, selected } => {
                let items: Vec<Line> = options
                    .iter()
                    .map(|choice| match choice {
                        Choice::State(state) => Line::from(state_spans(Some(state), false)),
                        Choice::Priority(priority) => {
                            Line::from(priority_span_long(*priority, false))
                        }
                    })
                    .collect();
                let list = List::new(items)
                    .highlight_symbol(">>")
                    .highlight_style(Style::default().bold());
                let mut state = ListState::default().with_selected(Some(*selected));
                StatefulWidget::render(list, body, buf, &mut state);
            }
            Input::Text { value, cursor } => {
                Paragraph::new(text_with_cursor(value, *cursor, body.width)).render(body, buf);
            }
        }

        if let Some(error) = dialog.error {
            let line = Rect {
                y: inner.y + inner.height.saturating_sub(1),
                height: 1,
                ..inner
            };
            Line::from(Span::from(error).fg(Color::LightRed)).render(line, buf);
        }
    }
}

/// The text with the character under the cursor drawn inverted, scrolled so
/// the cursor stays in view when the text is wider than the field.
fn text_with_cursor(value: &str, cursor: usize, width: u16) -> Line<'static> {
    let chars: Vec<char> = value.chars().collect();
    // Cells available, the cursor's own cell included.
    let visible = (width as usize).max(1);
    let start = (cursor + 1).saturating_sub(visible);

    let before: String = chars[start..cursor].iter().collect();
    let under = chars.get(cursor).map_or(" ".to_string(), char::to_string);
    let after: String = chars
        .iter()
        .skip(cursor + 1)
        .take(visible.saturating_sub(cursor - start + 1))
        .collect();

    Line::from(vec![
        Span::from(before).fg(Color::White),
        Span::from(under).reversed(),
        Span::from(after).fg(Color::White),
    ])
}

/// A `width` by `height` rectangle in the middle of `area`, shrunk to fit.
fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn the_cursor_past_the_end_shows_as_a_blank_cell() {
        assert_eq!(shown(&text_with_cursor("abc", 3, 20)), "abc ");
    }

    #[test]
    fn long_text_scrolls_to_keep_the_cursor_in_view() {
        let line = text_with_cursor("abcdefghij", 10, 5);
        assert_eq!(shown(&line), "ghij ");
    }

    #[test]
    fn text_after_the_cursor_is_cut_at_the_edge() {
        assert_eq!(shown(&text_with_cursor("abcdefghij", 0, 5)), "abcde");
    }

    #[test]
    fn the_dialog_shrinks_to_a_small_terminal() {
        let area = Rect::new(0, 0, 30, 4);
        assert_eq!(centred(area, 60, 10), area);
    }
}
