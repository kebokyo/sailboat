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
//! Editing one attribute of a work item.
//!
//! The details pane lists a work item's attributes, one per row. Pressing
//! Enter on an editable row opens a small dialog for it: a list to pick from
//! for state and priority, a line of text for the name and the dates. The
//! dialog turns what the user entered into an [`UpdateWorkItemRequest`] that
//! changes only that one attribute.

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent};

use crate::api::types::{Priority, StateLite, UpdateWorkItemRequest, WorkItem};

/// One row of the details pane, in the order the rows are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    Name,
    Key,
    State,
    Priority,
    Assignees,
    Labels,
    Start,
    Target,
    Estimate,
    Parent,
    Completed,
    Created,
    Updated,
}

impl Attribute {
    /// Every row, top to bottom. The details pane draws these and the selected
    /// row index maps straight back into this list.
    pub const ALL: [Attribute; 13] = [
        Attribute::Name,
        Attribute::Key,
        Attribute::State,
        Attribute::Priority,
        Attribute::Assignees,
        Attribute::Labels,
        Attribute::Start,
        Attribute::Target,
        Attribute::Estimate,
        Attribute::Parent,
        Attribute::Completed,
        Attribute::Created,
        Attribute::Updated,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Attribute::Name => "name",
            Attribute::Key => "key",
            Attribute::State => "state",
            Attribute::Priority => "priority",
            Attribute::Assignees => "assignees",
            Attribute::Labels => "labels",
            Attribute::Start => "start",
            Attribute::Target => "target",
            Attribute::Estimate => "estimate",
            Attribute::Parent => "parent",
            Attribute::Completed => "completed",
            Attribute::Created => "created",
            Attribute::Updated => "updated",
        }
    }
}

/// Every priority, most to least pressing, as the picker lists them.
const PRIORITIES: [Priority; 5] = [
    Priority::Urgent,
    Priority::High,
    Priority::Medium,
    Priority::Low,
    Priority::None,
];

/// An option in a picker.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    State(StateLite),
    Priority(Priority),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Choice {
        options: Vec<Choice>,
        selected: usize,
    },
    /// `cursor` counts characters, not bytes, so it never splits one.
    Text { value: String, cursor: usize },
}

/// What a keypress did to the dialog.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Still editing.
    Continue,
    /// Close without saving.
    Cancel,
    /// Close and send these changes. Boxed because the request is large next
    /// to the other variants.
    Submit(Box<UpdateWorkItemRequest>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditDialog {
    pub attribute: Attribute,
    pub input: Input,
    /// Why the last attempt to submit was refused, shown under the input.
    pub error: Option<String>,
}

impl EditDialog {
    /// A dialog for `attribute` on `item`, starting from its current value.
    ///
    /// `None` when the attribute can't be edited here, or when it is the state
    /// and the project's states have not loaded yet.
    pub fn open(attribute: Attribute, item: &WorkItem, states: &[StateLite]) -> Option<Self> {
        let input = match attribute {
            Attribute::Name => text(item.name.clone()),
            Attribute::Start => text(date_text(item.start_date)),
            Attribute::Target => text(date_text(item.target_date)),
            Attribute::State => {
                if states.is_empty() {
                    return None;
                }
                let current = item.state.as_ref().map(|state| state.id());
                Input::Choice {
                    selected: states
                        .iter()
                        .position(|state| Some(state.id) == current)
                        .unwrap_or(0),
                    options: states.iter().cloned().map(Choice::State).collect(),
                }
            }
            Attribute::Priority => Input::Choice {
                selected: PRIORITIES
                    .iter()
                    .position(|priority| *priority == item.priority)
                    .unwrap_or(0),
                options: PRIORITIES.into_iter().map(Choice::Priority).collect(),
            },
            _ => return None,
        };
        Some(EditDialog {
            attribute,
            input,
            error: None,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        match &mut self.input {
            Input::Choice { options, selected } => match key.code {
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Left => {
                    Outcome::Cancel
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = (*selected + 1).min(options.len().saturating_sub(1));
                    Outcome::Continue
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    Outcome::Continue
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    match options.get(*selected) {
                        Some(choice) => Outcome::Submit(Box::new(choice_changes(choice))),
                        None => Outcome::Cancel,
                    }
                }
                _ => Outcome::Continue,
            },
            // Every printable key types, so only Esc can back out of a text
            // field; Backspace deletes as it would anywhere else.
            Input::Text { value, cursor } => match key.code {
                KeyCode::Esc => Outcome::Cancel,
                KeyCode::Enter => match text_changes(self.attribute, value) {
                    Ok(changes) => Outcome::Submit(Box::new(changes)),
                    Err(error) => {
                        self.error = Some(error);
                        Outcome::Continue
                    }
                },
                _ => {
                    edit_text(value, cursor, key);
                    Outcome::Continue
                }
            },
        }
    }
}

/// Applies a typing or cursor key to a line of text. Shared by every text
/// field, so they all edit the same way.
pub(crate) fn edit_text(value: &mut String, cursor: &mut usize, key: KeyEvent) {
    match key.code {
        KeyCode::Char(ch) => {
            let at = byte_index(value, *cursor);
            value.insert(at, ch);
            *cursor += 1;
        }
        KeyCode::Backspace if *cursor > 0 => {
            *cursor -= 1;
            let at = byte_index(value, *cursor);
            value.remove(at);
        }
        KeyCode::Delete if *cursor < value.chars().count() => {
            let at = byte_index(value, *cursor);
            value.remove(at);
        }
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(value.chars().count()),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = value.chars().count(),
        _ => {}
    }
}

fn text(value: String) -> Input {
    let cursor = value.chars().count();
    Input::Text { value, cursor }
}

fn date_text(date: Option<NaiveDate>) -> String {
    date.map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn byte_index(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map_or(value.len(), |(index, _)| index)
}

fn choice_changes(choice: &Choice) -> UpdateWorkItemRequest {
    match choice {
        Choice::State(state) => UpdateWorkItemRequest {
            state: Some(state.id),
            ..Default::default()
        },
        Choice::Priority(priority) => UpdateWorkItemRequest {
            priority: Some(*priority),
            ..Default::default()
        },
    }
}

/// Checks what was typed and builds the change, or says what is wrong with it.
fn text_changes(attribute: Attribute, value: &str) -> Result<UpdateWorkItemRequest, String> {
    let value = value.trim();
    let date = || -> Result<Option<NaiveDate>, String> {
        // An emptied field clears the date.
        if value.is_empty() {
            return Ok(None);
        }
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| "dates are written YYYY-MM-DD".to_string())
    };

    Ok(match attribute {
        Attribute::Name if value.is_empty() => return Err("a work item needs a name".to_string()),
        Attribute::Name => UpdateWorkItemRequest {
            name: Some(value.to_string()),
            ..Default::default()
        },
        Attribute::Start => UpdateWorkItemRequest {
            start_date: Some(date()?),
            ..Default::default()
        },
        Attribute::Target => UpdateWorkItemRequest {
            target_date: Some(date()?),
            ..Default::default()
        },
        other => return Err(format!("{} can't be edited as text", other.label())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{Expandable, StateGroup};
    use crate::app::widgets::project_states::tests::state;

    fn item() -> WorkItem {
        serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::new_v4(),
            "name": "Fix the sails",
            "priority": "high",
            "target_date": "2026-11-01",
        }))
        .expect("minimal work item should decode")
    }

    fn press(dialog: &mut EditDialog, code: KeyCode) -> Outcome {
        dialog.handle_key(KeyEvent::from(code))
    }

    fn type_text(dialog: &mut EditDialog, text: &str) {
        for ch in text.chars() {
            press(dialog, KeyCode::Char(ch));
        }
    }

    #[test]
    fn only_some_rows_open_a_dialog() {
        let item = item();
        assert!(EditDialog::open(Attribute::Name, &item, &[]).is_some());
        assert!(EditDialog::open(Attribute::Priority, &item, &[]).is_some());
        assert!(EditDialog::open(Attribute::Key, &item, &[]).is_none());
        assert!(EditDialog::open(Attribute::Created, &item, &[]).is_none());
        // No states to pick from yet.
        assert!(EditDialog::open(Attribute::State, &item, &[]).is_none());
    }

    #[test]
    fn the_picker_starts_on_the_current_value() {
        let mut item = item();
        let states = vec![
            state("Todo", StateGroup::Unstarted),
            state("Doing", StateGroup::Started),
        ];
        item.state = Some(Expandable::Id(states[1].id));

        let dialog = EditDialog::open(Attribute::State, &item, &states).unwrap();
        assert!(matches!(dialog.input, Input::Choice { selected: 1, .. }));

        let dialog = EditDialog::open(Attribute::Priority, &item, &[]).unwrap();
        assert!(matches!(dialog.input, Input::Choice { selected: 1, .. }));
    }

    #[test]
    fn picking_a_state_sends_only_that_state() {
        let item = item();
        let states = vec![
            state("Todo", StateGroup::Unstarted),
            state("Done", StateGroup::Completed),
        ];
        let mut dialog = EditDialog::open(Attribute::State, &item, &states).unwrap();
        press(&mut dialog, KeyCode::Down);
        press(&mut dialog, KeyCode::Down); // Stops at the bottom.
        assert_eq!(
            press(&mut dialog, KeyCode::Enter),
            Outcome::Submit(Box::new(UpdateWorkItemRequest {
                state: Some(states[1].id),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn renaming_edits_the_current_name() {
        let mut dialog = EditDialog::open(Attribute::Name, &item(), &[]).unwrap();
        for _ in 0.."sails".len() {
            press(&mut dialog, KeyCode::Backspace);
        }
        type_text(&mut dialog, "hull");
        assert_eq!(
            press(&mut dialog, KeyCode::Enter),
            Outcome::Submit(Box::new(UpdateWorkItemRequest {
                name: Some("Fix the hull".to_string()),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn typing_q_in_a_text_field_types_it_rather_than_cancelling() {
        let mut dialog = EditDialog::open(Attribute::Name, &item(), &[]).unwrap();
        assert_eq!(press(&mut dialog, KeyCode::Char('q')), Outcome::Continue);
        assert_eq!(press(&mut dialog, KeyCode::Esc), Outcome::Cancel);
    }

    #[test]
    fn the_cursor_moves_by_characters_not_bytes() {
        let mut item = item();
        item.name = "café".to_string();
        let mut dialog = EditDialog::open(Attribute::Name, &item, &[]).unwrap();
        press(&mut dialog, KeyCode::Left);
        type_text(&mut dialog, "!");
        press(&mut dialog, KeyCode::End);
        press(&mut dialog, KeyCode::Backspace);
        assert!(matches!(&dialog.input, Input::Text { value, .. } if value == "caf!"));
    }

    #[test]
    fn an_empty_name_is_refused_and_the_dialog_stays_open() {
        let mut dialog = EditDialog::open(Attribute::Name, &item(), &[]).unwrap();
        press(&mut dialog, KeyCode::Home);
        for _ in 0..20 {
            press(&mut dialog, KeyCode::Delete);
        }
        assert_eq!(press(&mut dialog, KeyCode::Enter), Outcome::Continue);
        assert!(dialog.error.is_some());
    }

    #[test]
    fn clearing_a_date_sends_null_and_a_bad_date_is_refused() {
        let mut dialog = EditDialog::open(Attribute::Target, &item(), &[]).unwrap();
        for _ in 0.."2026-11-01".len() {
            press(&mut dialog, KeyCode::Backspace);
        }
        type_text(&mut dialog, "next week");
        assert_eq!(press(&mut dialog, KeyCode::Enter), Outcome::Continue);
        assert!(dialog.error.is_some());

        press(&mut dialog, KeyCode::Home);
        for _ in 0..20 {
            press(&mut dialog, KeyCode::Delete);
        }
        assert_eq!(
            press(&mut dialog, KeyCode::Enter),
            Outcome::Submit(Box::new(UpdateWorkItemRequest {
                target_date: Some(None),
                ..Default::default()
            }))
        );
    }
}
