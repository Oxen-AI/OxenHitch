//! Prints whether Claude's app icon can be read on this machine. Read-only.
use hitch_core::claude::{icon, ClaudePaths};

fn main() {
    let paths = ClaudePaths::detect();
    println!("icon file: {:?}", icon::icon_file(&paths));
    match icon::icon_data_url(&paths) {
        Some(url) => println!("data url: {} chars, starts {}", url.len(), &url[..40]),
        None => println!("data url: none"),
    }
}
