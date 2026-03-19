mod app;
mod cli;
mod crypto;
mod git_sync;
mod storage;
mod ui;
mod ui_api;
mod ui_assets;
mod ui_script;
mod ui_style;

pub fn run() -> anyhow::Result<()> {
    cli::run()
}
