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
//! A single project, fetched on its own.

use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::api::Client;
use crate::api::types::{DetailParams, Project};

use super::LoadingState;

/// The full record for one project, fetched on its own.
///
/// The row [`ListProjectsWidget`] already holds carries the same fields, so this
/// is not about getting data that is otherwise unavailable. What it buys is an
/// `expand=`, so the lead and default assignee arrive as people rather than bare
/// uuids, and the ability to refresh one project without pulling the whole
/// workspace listing again.
#[derive(Debug, Clone, Default)]
pub struct ProjectDetailsWidget {
    pub state: Arc<RwLock<ProjectDetailsState>>,
}

#[derive(Debug, Default)]
pub(crate) struct ProjectDetailsState {
    pub project: Option<Project>,
    pub loading_state: LoadingState,
    pub table_state: TableState,
    /// Which project this is showing, so a response that arrives after the user
    /// has moved on can be dropped instead of overwriting the current one.
    project_id: Option<Uuid>,
}

impl ProjectDetailsWidget {
    /// Point the widget at a project and fetch it.
    ///
    /// Switching to a different project clears first; refreshing the one already
    /// loaded keeps it on screen until its replacement arrives, matching how the
    /// two list widgets behave.
    pub(crate) fn run(&self, api: &Client, project_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            if state.project_id != Some(project_id) {
                state.project = None;
                state.project_id = Some(project_id);
            }
        }

        let this = self.clone();
        tokio::spawn(this.fetch_project(api.clone(), project_id));
    }

    /// Detached like its counterparts, so failures land in [`LoadingState::Error`].
    async fn fetch_project(self, api: Client, project_id: Uuid) {
        self.set_loading_state(LoadingState::Loading);
        match self.load_project(&api, project_id).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    /// The fallible half, split out so `?` has somewhere to go.
    async fn load_project(&self, api: &Client, project_id: Uuid) -> Result<()> {
        let params = DetailParams {
            // Ask for the people by name rather than by uuid. Plane answers an
            // unset relation here with `{}`, which the types module tolerates.
            expand: Some("project_lead,default_assignee".to_string()),
            ..Default::default()
        };

        let project = api.get_project(project_id, &params).await?;

        let mut state = self.state.write().unwrap();
        if state.project_id != Some(project_id) {
            return Ok(());
        }
        state.project = Some(project);
        Ok(())
    }

    pub(crate) fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    /// See [`ListProjectsWidget::is_ready`].
    pub fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }

    /// The loaded project, cloned so the caller is not holding a lock guard
    /// while it renders.
    pub fn project(&self) -> Option<Project> {
        self.state.read().unwrap().project.clone()
    }
}
