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
//! A single work item, fetched on its own.

use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::api::Client;
use crate::api::types::{DetailParams, WorkItem};

use super::LoadingState;

/// The full record for one work item.
///
/// Same shape as [`ProjectDetailsWidget`], but scoped by project as well: the
/// detail endpoint lives under a project, so both ids are needed to fetch one.
#[derive(Debug, Clone, Default)]
pub struct WorkItemDetailsWidget {
    pub state: Arc<RwLock<WorkItemDetailsState>>,
}

#[derive(Debug, Default)]
pub(crate) struct WorkItemDetailsState {
    pub work_item: Option<WorkItem>,
    pub loading_state: LoadingState,
    /// Which attribute row the cursor is on. A details table is worth making
    /// selectable because [`CurrentScreen::EditWorkItemDialog`] edits one
    /// attribute at a time -- the selected row is what it would open on.
    pub table_state: TableState,
    /// Which work item this is showing, so a response that arrives after the
    /// user has moved on can be dropped instead of overwriting the current one.
    work_item_id: Option<Uuid>,
}

impl WorkItemDetailsWidget {
    /// Point the widget at a work item and fetch it.
    ///
    /// Switching items clears first; refreshing the one already loaded keeps it
    /// on screen until its replacement arrives.
    pub(crate) fn run(&self, api: &Client, project_id: Uuid, work_item_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            if state.work_item_id != Some(work_item_id) {
                state.work_item = None;
                state.work_item_id = Some(work_item_id);
            }
        }

        let this = self.clone();
        tokio::spawn(this.fetch_work_item(api.clone(), project_id, work_item_id));
    }

    /// Detached like its counterparts, so failures land in [`LoadingState::Error`].
    async fn fetch_work_item(self, api: Client, project_id: Uuid, work_item_id: Uuid) {
        self.set_loading_state(LoadingState::Loading);
        match self.load_work_item(&api, project_id, work_item_id).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    /// The fallible half, split out so `?` has somewhere to go.
    async fn load_work_item(
        &self,
        api: &Client,
        project_id: Uuid,
        work_item_id: Uuid,
    ) -> Result<()> {
        let params = DetailParams {
            // Everything a details pane wants to name rather than show as a
            // uuid. Relations that are not set come back as stubs, which the
            // types module folds to `None`.
            expand: Some("state,parent,created_by,updated_by,estimate_point".to_string()),
            ..Default::default()
        };

        let work_item = api.get_work_item(project_id, work_item_id, &params).await?;

        let mut state = self.state.write().unwrap();
        if state.work_item_id != Some(work_item_id) {
            return Ok(());
        }
        state.work_item = Some(work_item);
        if state.table_state.selected().is_none() {
            state.table_state.select_first();
        }
        Ok(())
    }

    pub(crate) fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    /// See [`ListProjectsWidget::is_ready`].
    pub fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }

    /// The loaded work item, cloned so the caller is not holding a lock guard
    /// while it renders.
    pub fn work_item(&self) -> Option<WorkItem> {
        self.state.read().unwrap().work_item.clone()
    }

    pub fn scroll_down(&self) {
        self.state.write().unwrap().table_state.scroll_down_by(1);
    }

    pub fn scroll_up(&self) {
        self.state.write().unwrap().table_state.scroll_up_by(1);
    }
}
