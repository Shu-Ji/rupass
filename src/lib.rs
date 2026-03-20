mod app;
mod cli;
mod crypto;
mod git_sync;
mod storage;

pub fn run() -> anyhow::Result<()> {
    cli::run()
}
