//! Asks a gateway for its models with the key saved for that host, and prints the list
//! Hitch would write into Claude's config. Reads only; changes nothing.
//! Run with: cargo run -p hitch-core --example models -- localhost:3001

use hitch_core::claude::models::entries_for;
use hitch_core::gateway::{self, Check};
use hitch_core::secrets::{OxenConfig, SecretStore};

#[tokio::main]
async fn main() {
    let host = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hub.oxen.ai".into());
    let base_url = gateway::normalize_base_url(&host).expect("a host like hub.oxen.ai");
    let saved_under = base_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap();
    let Some(key) = OxenConfig::for_user().get(saved_under).ok().flatten() else {
        println!("No key saved for {saved_under} in the Oxen settings file.");
        return;
    };

    match gateway::check(&base_url, &key).await {
        Check::Ok { models } => {
            let with_uuid = models.iter().filter(|m| m.uuid.is_some()).count();
            println!(
                "{base_url}: {} chat models, {with_uuid} with a uuid",
                models.len()
            );
            for entry in entries_for(&[], &models) {
                println!("  {}", entry.to_config());
            }
        }
        other => println!("{base_url}: {other:?}"),
    }
}
