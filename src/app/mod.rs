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
use std::time::Duration;

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use ratatui::DefaultTerminal;
use tokio_stream::StreamExt;

pub mod edit;
pub mod widgets;

use crate::{
    api::{
        Client,
        types::{Project, StateGroup, UpdateWorkItemRequest, WorkItem},
    },
    config::Config,
    ui::ui,
};
use edit::{Attribute, EditDialog, Outcome};
use widgets::{
    ListProjectsWidget, ListWorkItemsWidget, ProjectDetailsWidget, ProjectStatesWidget,
    WorkItemDetailsWidget, project_states::closing_state,
};

/// The different views that sailboat displays.
#[derive(Debug, Default)]
pub enum CurrentScreen {
    /// The main view: projects on left, work items on right. Starts on the projects side.
    #[default]
    MainProjectsView,
    /// If the viewport is thin, the project list gets its own view.
    MainWorkItemsView,
    /// Viewing a specific work item's details.
    WorkItemDetailsView,
    /// Editing an attribute of a work item. Editing description is its own view: [`CurrentScreen::EditDescriptionView`]
    EditWorkItemDialog,
    /// Editing the description of a work item using Markdown.
    #[allow(dead_code)] // Rendered, but no key opens it yet.
    EditDescriptionView,
}

/// The main driver of sailboat's functionality.
#[derive(Debug, Default)]
pub struct App {
    pub current_workspace: String,
    pub current_project: Option<Project>,
    pub current_work_item: Option<WorkItem>,
    pub current_screen: CurrentScreen,

    pub list_projects: ListProjectsWidget,
    pub list_work_items: ListWorkItemsWidget,
    #[allow(dead_code)] // Not rendered yet; see `widgets::project_details`.
    pub project_details: ProjectDetailsWidget,
    pub work_item_details: WorkItemDetailsWidget,
    pub project_states: ProjectStatesWidget,
    /// The open edit dialog, while on [`CurrentScreen::EditWorkItemDialog`].
    pub edit_dialog: Option<EditDialog>,
    should_quit: bool,
}

impl App {
    const FRAMES_PER_SECOND: f32 = 60.0;

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let config = Config::load()?;
        let api = Client::new(&config)?;

        self.current_workspace = config.workspace.slug;

        self.list_projects.run(&api);

        let period = Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut events = EventStream::new();

        while !self.should_quit {
            tokio::select! {
                _ = interval.tick() => { terminal.draw(|frame| ui(frame, &self))?; },
                Some(Ok(event)) = events.next() => self.handle_event(&event, &api),
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, event: &Event, api: &Client) {
        if let Some(key) = event.as_key_press_event() {
            // The dialog takes every key: a text field needs `q` and Backspace
            // for typing, not for quitting and going back.
            if matches!(self.current_screen, CurrentScreen::EditWorkItemDialog) {
                self.handle_dialog_key(key, api);
                return;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
                KeyCode::Char('h') | KeyCode::Left => self.scroll_left(api),
                KeyCode::Char('l') | KeyCode::Right => self.scroll_right(api),
                KeyCode::Enter => self.confirm(api),
                KeyCode::Backspace => self.cancel(api),
                KeyCode::Char('x') => self.close_work_item(api),
                _ => {}
            }
        }
    }

    fn confirm(&mut self, api: &Client) {
        match self.current_screen {
            // Load the highlighted project's work items. Chosen because it
            // keeps fetches deliberate -- loading on every scroll step would
            // fire a request per keypress and burn the 60/minute budget.
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                if let Some(project) = self.list_projects.selected_project() {
                    self.list_work_items.run(api, project.id);
                    // Fetched now so the state picker and closing have them
                    // ready by the time a work item is open.
                    self.project_states.ensure(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            }

            // Open the highlighted work item. Needs the project id as well, so
            // it reads the project the pane is currently scoped to.
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                if let (Some(project), Some(item)) = (
                    self.current_project.as_ref(),
                    self.list_work_items.selected_work_item(),
                ) {
                    self.work_item_details.run(api, project.id, item.id);
                    // A no-op once loaded; retries if the fetch on opening the
                    // project failed.
                    self.project_states.ensure(api, project.id);
                    self.current_work_item = Some(item);
                    self.current_screen = CurrentScreen::WorkItemDetailsView;
                }
            }

            // Edit the attribute under the cursor, if it is one that can be.
            CurrentScreen::WorkItemDetailsView if self.work_item_details.is_ready() => {
                self.open_edit_dialog();
            }
            _ => {}
        }
    }

    fn cancel(&mut self, api: &Client) {
        match self.current_screen {
            CurrentScreen::MainWorkItemsView => {
                self.list_projects.run(api);
                // `current_project` is left as-is: the work items pane still
                // shows that project, and the highlight in the list survives the
                // refresh, so clearing it here would contradict both.
                self.current_screen = CurrentScreen::MainProjectsView;
            }
            // Return to the project the open work item belongs to. Reading the
            // project list's highlight instead could land on a different
            // project if the list was refreshed while the item was open.
            CurrentScreen::WorkItemDetailsView => {
                if let Some(project) = self.current_project.as_ref() {
                    self.list_work_items.run(api, project.id);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            }
            _ => {}
        }
    }

    // Scrolling and selection are ignored while the focused list is refreshing.
    // Its rows are about to be replaced, and acting on them risks opening a
    // project that no longer exists by the time the response lands.
    fn scroll_down(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                self.list_projects.scroll_down()
            }
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                self.list_work_items.scroll_down()
            }
            CurrentScreen::WorkItemDetailsView if self.work_item_details.is_ready() => {
                self.work_item_details.scroll_down()
            }
            _ => {}
        }
    }

    fn scroll_up(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                self.list_projects.scroll_up()
            }
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                self.list_work_items.scroll_up()
            }
            CurrentScreen::WorkItemDetailsView if self.work_item_details.is_ready() => {
                self.work_item_details.scroll_up()
            }
            _ => {}
        }
    }

    // h/← and l/→ are aliases for back and confirm, so the arrow keys and
    // Enter/Backspace always agree on where they lead.
    fn scroll_left(&mut self, api: &Client) {
        self.cancel(api);
    }

    fn scroll_right(&mut self, api: &Client) {
        self.confirm(api);
    }

    fn open_edit_dialog(&mut self) {
        let (Some(project), Some(item), Some(row)) = (
            self.current_project.as_ref(),
            self.work_item_details.work_item(),
            self.work_item_details.selected_row(),
        ) else {
            return;
        };
        let Some(attribute) = Attribute::ALL.get(row).copied() else {
            return;
        };
        let states = self.project_states.states(project.id);
        if let Some(dialog) = EditDialog::open(attribute, &item, &states) {
            self.edit_dialog = Some(dialog);
            self.current_screen = CurrentScreen::EditWorkItemDialog;
        }
    }

    fn handle_dialog_key(&mut self, key: KeyEvent, api: &Client) {
        let Some(dialog) = self.edit_dialog.as_mut() else {
            self.current_screen = CurrentScreen::WorkItemDetailsView;
            return;
        };
        match dialog.handle_key(key) {
            Outcome::Continue => return,
            Outcome::Cancel => {}
            Outcome::Submit(changes) => {
                if let (Some(project), Some(item)) = (
                    self.current_project.as_ref(),
                    self.current_work_item.as_ref(),
                ) {
                    self.work_item_details
                        .update(api, project.id, item.id, *changes);
                }
            }
        }
        self.edit_dialog = None;
        self.current_screen = CurrentScreen::WorkItemDetailsView;
    }

    /// Move a work item to the project's first completed state: the open one
    /// on the details screen, or the highlighted one in the work items list.
    /// Does nothing if it is already completed, or if the project's states
    /// have not loaded yet.
    fn close_work_item(&mut self, api: &Client) {
        let Some(project) = self.current_project.as_ref() else {
            return;
        };
        let states = self.project_states.states(project.id);
        let Some(done) = closing_state(&states) else {
            return;
        };
        let changes = UpdateWorkItemRequest {
            state: Some(done.id),
            ..Default::default()
        };
        let is_open = |item: &WorkItem| {
            let group = item
                .state
                .as_ref()
                .and_then(|state| states.iter().find(|s| s.id == state.id()))
                .map(|state| state.group);
            group != Some(StateGroup::Completed)
        };

        match self.current_screen {
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                if let Some(item) = self.list_work_items.selected_work_item()
                    && is_open(&item)
                {
                    self.list_work_items
                        .update(api, project.id, item.id, changes);
                }
            }
            CurrentScreen::WorkItemDetailsView if self.work_item_details.is_ready() => {
                if let Some(item) = self.work_item_details.work_item()
                    && is_open(&item)
                {
                    self.work_item_details
                        .update(api, project.id, item.id, changes);
                }
            }
            _ => {}
        }
    }

    pub fn work_item_key(&self) -> Option<String> {
        let project = self.current_project.as_ref()?;
        let item = self.current_work_item.as_ref()?;
        Some(format!("{}-{}", project.identifier, item.sequence_id))
    }

    pub fn work_item_name(&self) -> Option<String> {
        // Prefer the freshly fetched copy, so a rename shows up straight away.
        let item = self
            .work_item_details
            .work_item()
            .or_else(|| self.current_work_item.clone())?;
        Some(item.name)
    }
}
