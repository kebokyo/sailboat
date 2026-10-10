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
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Offset},
    prelude::Stylize,
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Tabs},
};

pub mod render;
pub mod widgets;

use widgets::{Focused, edit_work_item::DialogView, work_item_description::Description};

use crate::app::{App, CurrentScreen, edit::Input};

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

    let mut breadcrumbs = vec![app.current_workspace.clone()];
    match app.current_screen {
        CurrentScreen::WorkItemDetailsView
        | CurrentScreen::EditWorkItemDialog
        | CurrentScreen::EditDescriptionView => {
            if let Some(key) = app.work_item_key()
                && let Some(name) = app.work_item_name()
            {
                breadcrumbs.push(format!("{}: {}", key, name));
            }
        }
        /*CurrentScreen::EditDescriptionView => {
            if let Some(key) = app.work_item_key() {
                breadcrumbs.push(key);
                breadcrumbs.push("description".to_string())
            }
        }*/
        _ => {}
    }

    // Highlight the deepest crumb, which is where the user actually is.
    let deepest = breadcrumbs.len().saturating_sub(1);
    let tabs = Tabs::new(breadcrumbs)
        .style(Color::White)
        .highlight_style(Style::default().magenta().on_black().bold())
        .select(deepest)
        .divider(symbols::DOT)
        .padding(" ", " ");
    frame.render_widget(tabs, top_level[1] + Offset::new(1, 0));

    match app.current_screen {
        CurrentScreen::MainProjectsView
        | CurrentScreen::MainWorkItemsView
        | CurrentScreen::NewWorkItemDialog => {
            let main_content_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Fill(1)])
                .split(content_area);

            // Exactly one pane is live at a time; the other renders dimmed to show that
            // what it holds was fetched earlier and has not been refreshed since.
            let projects_focused = matches!(app.current_screen, CurrentScreen::MainProjectsView);
            let work_items_focused = matches!(
                app.current_screen,
                CurrentScreen::MainWorkItemsView | CurrentScreen::NewWorkItemDialog
            );

            frame.render_widget(
                Focused(&app.list_projects, projects_focused),
                main_content_layout[0],
            );
            frame.render_widget(
                Focused(&app.list_work_items, work_items_focused),
                main_content_layout[1],
            );

            if let Some(dialog) = &app.new_dialog {
                frame.render_widget(DialogView::new_work_item(dialog), content_area);
            }
        }
        CurrentScreen::WorkItemDetailsView
        | CurrentScreen::EditWorkItemDialog
        | CurrentScreen::EditDescriptionView => {
            let details_content_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Fill(1)])
                .split(content_area);

            // Exactly one pane is live at a time; the other renders dimmed to show that
            // what it holds was fetched earlier and has not been refreshed since.
            let details_focused = matches!(app.current_screen, CurrentScreen::WorkItemDetailsView);
            let description_focused =
                matches!(app.current_screen, CurrentScreen::EditDescriptionView);

            frame.render_widget(
                Focused(&app.work_item_details, details_focused),
                details_content_layout[0],
            );
            frame.render_widget(
                Description(&app.work_item_details, description_focused),
                details_content_layout[1],
            );

            if let Some(dialog) = &app.edit_dialog {
                frame.render_widget(DialogView::edit(dialog), content_area);
            }
        }
    }

    let mut keybinds: Vec<String> = vec![];
    match app.current_screen {
        CurrentScreen::MainProjectsView => {
            keybinds.push("j/k, ↓/↑ scroll".to_string());
            keybinds.push("l, →, ↳ open".to_string());
            keybinds.push("q, esc. quit".to_string());
        }
        CurrentScreen::MainWorkItemsView => {
            keybinds.push("j/k, ↓/↑ scroll".to_string());
            keybinds.push("l, →, ↳ open".to_string());
            keybinds.push("n new".to_string());
            keybinds.push("x close".to_string());
            keybinds.push("h, ←, ⌫ back".to_string());
            keybinds.push("q, esc. quit".to_string());
        }
        CurrentScreen::NewWorkItemDialog => {
            keybinds.push("type a name".to_string());
            keybinds.push("↳ create".to_string());
            keybinds.push("esc. cancel".to_string());
        }
        CurrentScreen::WorkItemDetailsView => {
            keybinds.push("j/k, ↓/↑ scroll".to_string());
            keybinds.push("l, →, ↳ edit".to_string());
            keybinds.push("x close".to_string());
            keybinds.push("h, ←, ⌫ back".to_string());
            keybinds.push("q, esc. quit".to_string());
        }
        CurrentScreen::EditWorkItemDialog => match app.edit_dialog.as_ref().map(|d| &d.input) {
            Some(Input::Text { .. }) => {
                keybinds.push("type to edit".to_string());
                keybinds.push("↳ save".to_string());
                keybinds.push("esc. cancel".to_string());
            }
            _ => {
                keybinds.push("j/k, ↓/↑ choose".to_string());
                keybinds.push("l, →, ↳ save".to_string());
                keybinds.push("h, ←, ⌫, esc. cancel".to_string());
            }
        },
        CurrentScreen::EditDescriptionView => {
            keybinds.push("←/↓/↑/→ navigate".to_string());
            keybinds.push("⇥ attributes".to_string());
            keybinds.push("^+s save".to_string());
            keybinds.push("⌃+↳ confirm".to_string());
            keybinds.push("^+⌫ cancel".to_string());
            keybinds.push("esc. quit".to_string());
        }
    }

    let footer = Line::from(Span::from(keybinds.join(" | ")).fg(Color::White));
    frame.render_widget(footer, top_level[2] + Offset::new(1, 0));
}
