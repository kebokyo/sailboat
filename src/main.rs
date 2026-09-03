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

mod api;
mod config;
mod ui;
mod app;

use crate::app::App;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app_result = App::default().run(terminal).await;
    ratatui::restore();
    app_result
}
