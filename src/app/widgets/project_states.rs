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
//! The states a project's work items can be in, cached per project.
//!
//! Changing a work item's state, and closing one, both need to know which
//! states exist and which group each belongs to. The list is fetched once when
//! a project is opened and reused for every work item in it.

use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use uuid::Uuid;

use crate::api::Client;
use crate::api::types::{PageParams, StateGroup, StateLite};

use super::LoadingState;

#[derive(Debug, Clone, Default)]
pub struct ProjectStatesWidget {
    pub state: Arc<RwLock<ProjectStatesState>>,
}

#[derive(Debug, Default)]
pub(crate) struct ProjectStatesState {
    pub states: Vec<StateLite>,
    pub loading_state: LoadingState,
    /// Which project these belong to, so a response for a project the user has
    /// already left can be dropped.
    project_id: Option<Uuid>,
}

impl ProjectStatesWidget {
    /// Make sure the states for `project_id` are loaded, fetching them only if
    /// they are not already. Cheap to call on every navigation.
    pub(crate) fn ensure(&self, api: &Client, project_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            if state.project_id == Some(project_id) {
                match state.loading_state {
                    LoadingState::Loaded | LoadingState::Loading => return,
                    // A failed fetch is retried on the next navigation.
                    LoadingState::Idle | LoadingState::Error(_) => {}
                }
            } else {
                state.states.clear();
                state.project_id = Some(project_id);
            }
            state.loading_state = LoadingState::Loading;
        }

        let this = self.clone();
        tokio::spawn(this.fetch_states(api.clone(), project_id));
    }

    async fn fetch_states(self, api: Client, project_id: Uuid) {
        let result = self.load_states(&api, project_id).await;
        let mut state = self.state.write().unwrap();
        if state.project_id != Some(project_id) {
            return;
        }
        state.loading_state = match result {
            Ok(states) => {
                state.states = states;
                LoadingState::Loaded
            }
            Err(error) => LoadingState::Error(error.to_string()),
        };
    }

    async fn load_states(&self, api: &Client, project_id: Uuid) -> Result<Vec<StateLite>> {
        let params = PageParams {
            // Projects have a handful of states; one page covers them all.
            per_page: Some(100),
            ..Default::default()
        };
        let page = api.list_states(project_id, &params).await?;
        Ok(pickable_states(page.results))
    }

    /// The states for `project_id`, or nothing if they have not loaded yet.
    pub(crate) fn states(&self, project_id: Uuid) -> Vec<StateLite> {
        let state = self.state.read().unwrap();
        if state.project_id == Some(project_id) && state.loading_state == LoadingState::Loaded {
            state.states.clone()
        } else {
            Vec::new()
        }
    }
}

/// Orders states the way a work item moves through them, and drops triage:
/// items in a triage state are hidden from the work item list, so moving one
/// there would make it vanish.
fn pickable_states(mut states: Vec<StateLite>) -> Vec<StateLite> {
    states.retain(|state| state.group != StateGroup::Triage);
    // Stable, so states within a group keep the order Plane returned them in.
    states.sort_by_key(|state| group_order(state.group));
    states
}

fn group_order(group: StateGroup) -> u8 {
    match group {
        StateGroup::Backlog => 0,
        StateGroup::Unstarted => 1,
        StateGroup::Started => 2,
        StateGroup::Completed => 3,
        StateGroup::Cancelled => 4,
        StateGroup::Triage => 5,
    }
}

/// The state closing a work item moves it to: the first one in the completed
/// group.
pub(crate) fn closing_state(states: &[StateLite]) -> Option<&StateLite> {
    states
        .iter()
        .find(|state| state.group == StateGroup::Completed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn state(name: &str, group: StateGroup) -> StateLite {
        StateLite {
            id: Uuid::new_v4(),
            name: name.to_string(),
            color: String::new(),
            group,
        }
    }

    #[test]
    fn states_are_ordered_by_group_and_triage_is_dropped() {
        let states = vec![
            state("Done", StateGroup::Completed),
            state("Intake", StateGroup::Triage),
            state("Todo", StateGroup::Unstarted),
            state("Backlog", StateGroup::Backlog),
            state("Doing", StateGroup::Started),
            state("Review", StateGroup::Started),
        ];
        let names: Vec<String> = pickable_states(states)
            .into_iter()
            .map(|state| state.name)
            .collect();
        assert_eq!(names, ["Backlog", "Todo", "Doing", "Review", "Done"]);
    }

    #[test]
    fn closing_picks_the_first_completed_state() {
        let states = vec![
            state("Todo", StateGroup::Unstarted),
            state("Cancelled", StateGroup::Cancelled),
            state("Done", StateGroup::Completed),
            state("Shipped", StateGroup::Completed),
        ];
        assert_eq!(closing_state(&states).unwrap().name, "Done");
        assert!(closing_state(&states[..2]).is_none());
    }
}
