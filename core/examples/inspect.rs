//! Prints what Hitch sees on this machine. Reads only; changes nothing.
//! Run with: cargo run -p hitch-core --example inspect

use hitch_core::claude::process::SystemClaude;
use hitch_core::claude::{library, ClaudeApp, ClaudePaths};
use hitch_core::secrets::{OxenConfig, SecretStore};

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

    // Which hosts the Oxen CLI has a key for. Only the last four characters are shown.
    let keys = OxenConfig::for_user();
    for host in ["hub.oxen.ai", "localhost:3001", "dev.hub.oxen.ai"] {
        let hint = keys.get(host).ok().flatten().map(|key| {
            let tail: String = key
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("saved, ends in {tail}")
        });
        println!("key for {host}: {}", hint.unwrap_or_else(|| "none".into()));
    }
}
