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
//! Creating a work item.
//!
//! `n` on the work items list asks for a name and creates the item with the
//! project's defaults for everything else: its default state, no priority, and
//! its default assignee. The rest can be set afterwards from the details
//! screen.

use crossterm::event::{KeyCode, KeyEvent};

use crate::api::types::CreateWorkItemRequest;

use super::edit::{Input, edit_text};

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Continue,
    Cancel,
    Submit(Box<CreateWorkItemRequest>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewWorkItemDialog {
    /// Always [`Input::Text`]; kept as an [`Input`] so it draws like the edit
    /// dialog.
    pub input: Input,
    pub error: Option<String>,
}

impl Default for NewWorkItemDialog {
    fn default() -> Self {
        NewWorkItemDialog {
            input: Input::Text {
                value: String::new(),
                cursor: 0,
            },
            error: None,
        }
    }
}

impl NewWorkItemDialog {
    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        let Input::Text { value, cursor } = &mut self.input else {
            return Outcome::Cancel;
        };
        match key.code {
            KeyCode::Esc => Outcome::Cancel,
            KeyCode::Enter => {
                let name = value.trim();
                if name.is_empty() {
                    self.error = Some("a work item needs a name".to_string());
                    return Outcome::Continue;
                }
                Outcome::Submit(Box::new(CreateWorkItemRequest {
                    name: name.to_string(),
                    ..Default::default()
                }))
            }
            _ => {
                edit_text(value, cursor, key);
                Outcome::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(dialog: &mut NewWorkItemDialog, code: KeyCode) -> Outcome {
        dialog.handle_key(KeyEvent::from(code))
    }

    #[test]
    fn the_typed_name_is_sent_trimmed_with_nothing_else_set() {
        let mut dialog = NewWorkItemDialog::default();
        for ch in "  Rig the mast ".chars() {
            press(&mut dialog, KeyCode::Char(ch));
        }
        assert_eq!(
            press(&mut dialog, KeyCode::Enter),
            Outcome::Submit(Box::new(CreateWorkItemRequest {
                name: "Rig the mast".to_string(),
                ..Default::default()
            }))
        );
    }

    #[test]
    fn a_blank_name_is_refused_and_esc_backs_out() {
        let mut dialog = NewWorkItemDialog::default();
        press(&mut dialog, KeyCode::Char(' '));
        assert_eq!(press(&mut dialog, KeyCode::Enter), Outcome::Continue);
        assert!(dialog.error.is_some());
        assert_eq!(press(&mut dialog, KeyCode::Esc), Outcome::Cancel);
    }
}
