//! Restarts the real Claude the way the Restart button does, and says what happened.
//! `cargo run -p hitch-core --example restart`

use hitch_core::claude::process::SystemClaude;
use hitch_core::claude::{ClaudeApp, ClaudePaths};

fn main() {
    let claude = SystemClaude {
        bundles: ClaudePaths::detect().app_bundles,
    };
    println!("running since {:?}", claude.started_at());
    let began = std::time::Instant::now();
    match claude.restart() {
        Ok(()) => println!("restarted in {:.1?}", began.elapsed()),
        Err(err) => println!("failed after {:.1?}: {err}", began.elapsed()),
    }
    println!("running since {:?}", claude.started_at());
}
