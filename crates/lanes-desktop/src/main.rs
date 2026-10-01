slint::include_modules!();

use lanes_core::{detect, discover, launch, process_alive, stop, Result};
use slint::{ModelRc, VecModel};
use std::path::Path;

fn refresh(ui: &AppWindow) {
    let path = ui.get_project_path().to_string();
    match discover(Path::new(&path)) {
        Ok(lanes) => {
            let rows = lanes
                .iter()
                .map(|lane| LaneRow {
                    branch: lane.branch.clone().into(),
                    url: lane.url().into(),
                    state: if process_alive(lane) {
                        "● running"
                    } else {
                        "○ stopped"
                    }
                    .into(),
                    path: lane.path.to_string_lossy().into_owned().into(),
                    port: lane.port.to_string().into(),
                })
                .collect::<Vec<_>>();
            ui.set_lanes(ModelRc::new(VecModel::from(rows)));
            ui.set_message(format!("{} worktrees · 0 port collisions", lanes.len()).into());
        }
        Err(error) => ui.set_message(format!("{error}").into()),
    }
}

fn run_lane(path: &str, command: &str) -> Result<()> {
    let worktree = detect(Path::new(path))?;
    let args = shell_words::split(command)?;
    let (lane, _child) = launch(&worktree, &args, false)?;
    println!("Started {} at {}", lane.branch, lane.url());
    Ok(())
}

fn main() -> Result<()> {
    let ui = AppWindow::new()?;
    ui.set_project_path(
        std::env::current_dir()?
            .to_string_lossy()
            .into_owned()
            .into(),
    );

    let weak = ui.as_weak();
    ui.on_refresh(move || {
        if let Some(ui) = weak.upgrade() {
            refresh(&ui);
        }
    });

    let weak = ui.as_weak();
    ui.on_run_lane(move |path| {
        if let Some(ui) = weak.upgrade() {
            if let Err(error) = run_lane(&path, &ui.get_command_text()) {
                ui.set_message(format!("{error}").into());
            } else {
                refresh(&ui);
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_stop_lane(move |path| {
        if let Some(ui) = weak.upgrade() {
            if let Err(error) = stop(Path::new(path.as_str())) {
                ui.set_message(format!("{error}").into());
            } else {
                refresh(&ui);
            }
        }
    });

    refresh(&ui);
    ui.run()?;
    Ok(())
}
