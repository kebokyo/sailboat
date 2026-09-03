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
//! One project's work items.

use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::api::Client;
use crate::api::types::{ListWorkItemsParams, WorkItem};

use super::{LoadingState, restored_selection};

/// The work items belonging to one project.
///
/// Mirrors [`ListProjectsWidget`], with the difference that its contents are
/// scoped to a project and are replaced wholesale when the selection changes.
#[derive(Debug, Clone, Default)]
pub struct ListWorkItemsWidget {
    pub state: Arc<RwLock<ListWorkItemsState>>,
}

#[derive(Debug, Default)]
pub(crate) struct ListWorkItemsState {
    pub work_items: Vec<WorkItem>,
    pub loading_state: LoadingState,
    pub table_state: TableState,
    /// Which project these belong to, so paging can continue against it.
    project_id: Option<Uuid>,
    cursor: Option<String>,
}

impl ListWorkItemsWidget {
    /// Point the widget at a project and fetch its first page.
    ///
    /// Switching to a *different* project wipes the pane first, since showing
    /// one project's items under another's heading would be wrong. Refreshing
    /// the project already on screen leaves the rows in place until their
    /// replacements arrive, so the pane never blanks.
    pub(crate) fn run(&self, api: &Client, project_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            if state.project_id != Some(project_id) {
                state.work_items.clear();
                state.cursor = None;
                state.table_state.select(None);
                state.project_id = Some(project_id);
            }
        }

        let this = self.clone();
        tokio::spawn(this.fetch_work_items(api.clone(), project_id));
    }

    /// Detached like its counterpart, so failures land in [`LoadingState::Error`].
    async fn fetch_work_items(self, api: Client, project_id: Uuid) {
        self.set_loading_state(LoadingState::Loading);
        match self.load_work_items(&api, project_id).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    async fn load_work_items(&self, api: &Client, project_id: Uuid) -> Result<()> {
        let mut params = ListWorkItemsParams {
            // Inline everything a row shows, so no column costs a request of
            // its own.
            expand: Some("state,assignees,labels".to_string()),
            ..Default::default()
        };
        params.cursor = self.state.read().unwrap().cursor.clone();

        let page = api.list_work_items(project_id, &params).await?;
        let next_cursor = page.next().map(str::to_string);

        let mut state = self.state.write().unwrap();
        // The selection may have moved on while this was in flight; anything that
        // arrives for a project we are no longer showing gets dropped.
        if state.project_id != Some(project_id) {
            return Ok(());
        }
        // Same reasoning as the projects list: swap the contents in one
        // assignment and put the cursor back on the work item it was on.
        let selected_id = state
            .table_state
            .selected()
            .and_then(|index| state.work_items.get(index))
            .map(|item| item.id);

        state.work_items = page.results;
        state.cursor = next_cursor;

        let next_selection = restored_selection(&state.work_items, selected_id);
        state.table_state.select(next_selection);
        Ok(())
    }

    pub(crate) fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    pub fn scroll_down(&self) {
        self.state.write().unwrap().table_state.scroll_down_by(1);
    }

    pub fn scroll_up(&self) {
        self.state.write().unwrap().table_state.scroll_up_by(1);
    }

    /// See [`ListProjectsWidget::is_ready`].
    pub(crate) fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }

    /// The work item under the cursor. Cloned rather than borrowed so the caller
    /// is not holding a lock guard while it decides what to do next.
    pub(crate) fn selected_work_item(&self) -> Option<WorkItem> {
        let state = self.state.read().unwrap();
        state
            .table_state
            .selected()
            .and_then(|index| state.work_items.get(index))
            .cloned()
    }
}
