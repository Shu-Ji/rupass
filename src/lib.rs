mod app;
mod cli;
mod crypto;
mod git_sync;
mod s3_sync;
mod storage;
mod team_sync;
mod tui;
mod tui_actions;
mod tui_app;
mod tui_ops;
mod tui_style;
mod tui_view;

pub fn run() -> anyhow::Result<()> {
    cli::run()
}
