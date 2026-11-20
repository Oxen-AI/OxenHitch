//! Prints what Hitch sees on this machine. Reads only; changes nothing.
//! Run with: cargo run -p hitch-core --example inspect

use hitch_core::claude::process::SystemClaude;
use hitch_core::claude::{library, ClaudeApp, ClaudePaths};

fn main() {
    let paths = ClaudePaths::detect();
    let claude = SystemClaude {
        bundles: paths.app_bundles.clone(),
    };
    println!("data dir:    {}", paths.data_dir.display());
    println!("installed:   {}", claude.installed());
    println!("started at:  {:?}", claude.started_at());
    println!("managed:     {}", paths.managed());
    let snapshot = library::snapshot(&paths);
    println!("applied id:  {:?}", snapshot.applied_id);
    println!("entries:     {:?}", snapshot.entry_ids);
    println!("mode:        {:?}", snapshot.mode);
    println!("applied:     {:?}", snapshot.applied);
}
