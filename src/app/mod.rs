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
use crossterm::event::{Event, EventStream, KeyCode};
use ratatui::DefaultTerminal;
use tokio_stream::StreamExt;


pub mod widgets;

use crate::{
    api::{Client, types::{Project, WorkItem}},
    config::Config,
    ui::ui,
};
use widgets::{
    ListProjectsWidget, ListWorkItemsWidget, ProjectDetailsWidget, WorkItemDetailsWidget,
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
    pub project_details: ProjectDetailsWidget,
    pub work_item_details: WorkItemDetailsWidget,
    should_quit: bool
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
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
                KeyCode::Char('h') | KeyCode::Left => self.scroll_left(&api),
                KeyCode::Char('l') | KeyCode::Right => self.scroll_right(&api),
                KeyCode::Enter => self.confirm(&api),
                KeyCode::Backspace => self.cancel(&api),
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
                    //self.project_details.run(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            },

            // Open the highlighted work item. Needs the project id as well, so
            // it reads the project the pane is currently scoped to.
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                if let (Some(project), Some(item)) = (
                    self.current_project.as_ref(),
                    self.list_work_items.selected_work_item(),
                ) {
                    self.work_item_details.run(api, project.id, item.id);
                    self.current_work_item = Some(item);
                    self.current_screen = CurrentScreen::WorkItemDetailsView;
                }
            },
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
            },
            CurrentScreen::WorkItemDetailsView => {
                if let Some(project) = self.list_projects.selected_project() {
                    self.list_work_items.run(api, project.id);
                    //self.project_details.run(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            },
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

    fn scroll_left(&mut self, api: &Client) {
        match self.current_screen {
            CurrentScreen::MainWorkItemsView => {
                self.list_projects.run(api);
                // `current_project` is left as-is: the work items pane still
                // shows that project, and the highlight in the list survives the
                // refresh, so clearing it here would contradict both.
                self.current_screen = CurrentScreen::MainProjectsView;
            }
            _ => {}
        }
    }
    
    fn scroll_right(&mut self, api: &Client) {
        match self.current_screen {
            // Load the highlighted project's work items. Chosen because it
            // keeps fetches deliberate -- loading on every scroll step would
            // fire a request per keypress and burn the 60/minute budget.
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                if let Some(project) = self.list_projects.selected_project() {
                    self.list_work_items.run(api, project.id);
                    self.project_details.run(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            },

            // Open the highlighted work item. Needs the project id as well, so
            // it reads the project the pane is currently scoped to.
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                if let (Some(project), Some(item)) = (
                    self.current_project.as_ref(),
                    self.list_work_items.selected_work_item(),
                ) {
                    self.work_item_details.run(api, project.id, item.id);
                    self.current_work_item = Some(item);
                    self.current_screen = CurrentScreen::WorkItemDetailsView;
                }
            },
            _ => {}
        }
    }

    pub fn work_item_key(&self) -> Option<String> {
        let project = self.current_project.as_ref()?;
        let item = self.current_work_item.as_ref()?;
        Some(format!("{}-{}", project.identifier, item.sequence_id))
    }

    pub fn work_item_name(&self) -> Option<String> {
        let item = self.current_work_item.as_ref()?;
        Some(item.name.clone())
    }
}
