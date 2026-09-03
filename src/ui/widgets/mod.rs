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
//! The render half of each widget.
//!
//! Every pane is drawn through [`Focused`], which pairs a widget with whether it
//! currently has focus. Colour is the signal that a pane holds data the user is
//! driving right now; grey means the contents were fetched earlier and have not
//! been refreshed since, and it stays grey until that pane is focused *and* its
//! refresh lands.

use ratatui::style::Color;

pub mod list_projects;
pub mod list_work_items;
pub mod work_item_description;
pub mod work_item_details;

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

use ratatui::text::Span;
use ratatui::prelude::Stylize;

use crate::api::types::{Expandable, LabelLite, Priority, StateGroup, StateLite, UserLite};

/// Strip variation selectors from text before it reaches a cell.
///
/// `U+FE0F` asks for emoji presentation, which terminals draw two cells wide.
/// The width tables ratatui measures with only see the base character, and for
/// something like `⚙` (U+2699, east-asian-width Neutral) that is one cell -- so
/// ratatui reserves one cell for a glyph the terminal paints across two, and the
/// highlight stops halfway through. Dropping the selector asks for text
/// presentation instead, which is the width that was actually measured.
///
/// The trade is a monochrome glyph rather than a colour one. Terminals that
/// render colour emoji regardless of the selector will still misalign, and
/// zero-width-joiner sequences (`👨‍💻`) have the inverse problem -- measured wide,
/// drawn narrow -- which nothing at this layer can fix.
pub(crate) fn display_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '\u{FE0F}' | '\u{FE0E}'))
        .collect()
}

/// Plane stores label colours as `#rrggbb`.
fn hex_colour(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    Some(Color::Rgb(
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

/// Glyph and colour for a work item's priority.
///
/// Shared so the list and the details pane cannot drift apart; they are the same
/// fact shown in two places.
pub(crate) fn priority_marks(priority: Priority) -> (&'static str, Color) {
    match priority {
        Priority::Urgent => ("!", Color::LightRed),
        Priority::High => ("^", Color::LightYellow),
        Priority::Medium => ("-", Color::White),
        Priority::Low => ("v", Color::LightBlue),
        Priority::None => (" ", Color::DarkGray),
    }
}

pub(crate) fn priority_span(priority: Priority, dimmed: bool) -> Span<'static> {
    let (glyph, colour) = priority_marks(priority);
    Span::from(glyph).fg(if dimmed { Color::DarkGray } else { colour })
}

pub(crate) fn priority_span_long(priority: Priority, dimmed: bool) -> Span<'static> {
    let (glyph, colour) = priority_marks(priority);
    let text = format!("{} {:?}", glyph, priority).to_lowercase();
    Span::from(text.clone()).fg(if dimmed { Color::DarkGray } else { colour })
}

/// Glyph and colour for a state, chosen by its group.
///
/// The group is what carries meaning -- a project can rename "Done" to anything
/// -- so the icon tracks the group rather than the state's name. Circles fill in
/// as work progresses, which reads as a rough progress bar down a column.
pub(crate) fn state_marks(group: StateGroup) -> (&'static str, Color) {
    match group {
        StateGroup::Backlog => ("◌", Color::DarkGray),
        StateGroup::Unstarted => ("○", Color::Gray),
        StateGroup::Started => ("◐", Color::LightYellow),
        StateGroup::Completed => ("●", Color::LightGreen),
        StateGroup::Cancelled => ("⊘", Color::LightRed),
        StateGroup::Triage => ("◈", Color::LightMagenta),
    }
}

/// A state as `icon name`, or a placeholder when the relation was not expanded.
pub(crate) fn state_spans(state: Option<&StateLite>, dimmed: bool) -> Vec<Span<'static>> {
    match state {
        Some(state) => {
            let (glyph, colour) = state_marks(state.group);
            let colour = if dimmed { Color::DarkGray } else { colour };
            vec![
                Span::from(format!("{glyph} ")).fg(colour),
                Span::from(display_text(&state.name)).fg(colour),
            ]
        }
        None => vec![Span::from("").fg(Color::DarkGray)],
    }
}

/// Labels as their own names in their own colours, comma-separated.
///
/// Falls back to a count when the relation was not expanded, since a list of
/// uuids tells the reader nothing.
pub(crate) fn label_spans(
    labels: &Option<Vec<Expandable<LabelLite>>>,
    dimmed: bool,
) -> Vec<Span<'static>> {
    let Some(labels) = labels.as_ref().filter(|labels| !labels.is_empty()) else {
        return Vec::new();
    };

    let expanded: Vec<&LabelLite> = labels.iter().filter_map(|l| l.expanded()).collect();
    if expanded.is_empty() {
        return vec![Span::from(format!("{} label(s)", labels.len())).fg(Color::DarkGray)];
    }

    let mut spans = Vec::new();
    for (index, label) in expanded.iter().enumerate() {
        if index > 0 {
            spans.push(Span::from(" ").fg(Color::DarkGray));
        }
        let colour = if dimmed {
            Color::DarkGray
        } else {
            label
                .color
                .as_deref()
                .and_then(hex_colour)
                .unwrap_or(Color::Gray)
        };
        spans.push(Span::from(format!("⌗{}", display_text(&label.name))).fg(colour));
    }
    spans
}

/// Assignees as `@name`, or a count when the relation was not expanded.
pub(crate) fn assignee_spans(
    assignees: &Option<Vec<Expandable<UserLite>>>,
    dimmed: bool,
) -> Vec<Span<'static>> {
    let Some(assignees) = assignees.as_ref().filter(|list| !list.is_empty()) else {
        return Vec::new();
    };
    let colour = if dimmed { Color::DarkGray } else { Color::Cyan };

    let names: Vec<String> = assignees
        .iter()
        .map(|user| match user.expanded() {
            Some(user) => format!(
                "@{}",
                user.display_name
                    .clone()
                    .or_else(|| user.first_name.clone())
                    .or_else(|| user.email.clone())
                    .unwrap_or_else(|| "someone".to_string())
            ),
            None => "@?".to_string(),
        })
        .collect();

    vec![Span::from(names.join(" ")).fg(colour)]
}
