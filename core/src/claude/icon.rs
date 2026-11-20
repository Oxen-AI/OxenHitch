//! Claude's own app icon, read from the copy installed on this machine. Hitch shows it
//! next to Oxen's mark, so the picture is always the app the user actually has and no
//! Anthropic artwork ships inside Hitch.

use std::fs;
use std::path::{Path, PathBuf};

use super::ClaudePaths;

/// The icon file of the first installed Claude bundle that has one.
pub fn icon_file(paths: &ClaudePaths) -> Option<PathBuf> {
    paths
        .app_bundles
        .iter()
        .find_map(|bundle| bundle_icon(bundle))
}

/// The icon as a `data:` URL holding a small PNG, ready for an `<img>`. `None` when
/// Claude is not installed or the icon cannot be read.
pub fn icon_data_url(paths: &ClaudePaths) -> Option<String> {
    let png = to_png(&icon_file(paths)?)?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

/// The file `CFBundleIconFile` names in the bundle's `Info.plist`.
fn bundle_icon(bundle: &Path) -> Option<PathBuf> {
    let contents = bundle.join("Contents");
    let info = fs::read_to_string(contents.join("Info.plist")).ok()?;
    let after_key = info.split("<key>CFBundleIconFile</key>").nth(1)?;
    let name = after_key
        .split("<string>")
        .nth(1)?
        .split("</string>")
        .next()?
        .trim();
    let mut file = contents.join("Resources").join(name);
    if file.extension().is_none() {
        file.set_extension("icns");
    }
    file.is_file().then_some(file)
}

/// Pixels on the long side. The icon is drawn at about 50 points, so this stays sharp
/// on a Retina display without sending a full-size icon to the window.
const SIZE: &str = "128";

/// macOS ships `sips`, which reads `.icns`. Other platforms have no icon yet.
#[cfg(target_os = "macos")]
fn to_png(icns: &Path) -> Option<Vec<u8>> {
    let out = std::env::temp_dir().join(format!("hitch-claude-icon-{}.png", uuid::Uuid::new_v4()));
    let converted = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-Z", SIZE])
        .arg(icns)
        .arg("--out")
        .arg(&out)
        .output()
        .is_ok_and(|done| done.status.success());
    let png = converted.then(|| fs::read(&out).ok()).flatten();
    let _ = fs::remove_file(&out);
    png.filter(|bytes| !bytes.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn to_png(_icns: &Path) -> Option<Vec<u8>> {
    let _ = SIZE;
    None
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .fold(0u32, |n, byte| (n << 8) | u32::from(*byte))
            << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_with(dir: &Path, info: &str, icons: &[&str]) -> ClaudePaths {
        let contents = dir.join("Claude.app/Contents");
        fs::create_dir_all(contents.join("Resources")).unwrap();
        fs::write(contents.join("Info.plist"), info).unwrap();
        for icon in icons {
            fs::write(contents.join("Resources").join(icon), b"icns").unwrap();
        }
        let mut paths = ClaudePaths::at(dir.join("data"));
        paths.app_bundles = vec![dir.join("Missing.app"), dir.join("Claude.app")];
        paths
    }

    const INFO: &str = "<plist><dict>\n<key>CFBundleIconFile</key>\n\t<string>electron.icns</string>\n</dict></plist>";

    #[test]
    fn finds_the_icon_the_bundle_names() {
        let dir = tempfile::tempdir().unwrap();
        let paths = bundle_with(dir.path(), INFO, &["other.icns", "electron.icns"]);
        assert_eq!(
            icon_file(&paths),
            Some(
                dir.path()
                    .join("Claude.app/Contents/Resources/electron.icns")
            )
        );
    }

    #[test]
    fn adds_the_extension_a_bundle_may_leave_off() {
        let dir = tempfile::tempdir().unwrap();
        let info = INFO.replace("electron.icns", "electron");
        let paths = bundle_with(dir.path(), &info, &["electron.icns"]);
        assert_eq!(
            icon_file(&paths),
            Some(
                dir.path()
                    .join("Claude.app/Contents/Resources/electron.icns")
            )
        );
    }

    #[test]
    fn no_icon_when_claude_is_not_installed_or_names_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(icon_file(&ClaudePaths::at(dir.path())), None);
        let paths = bundle_with(dir.path(), INFO, &[]);
        assert_eq!(icon_file(&paths), None);
        assert_eq!(icon_data_url(&paths), None);
    }

    #[test]
    fn encodes_base64_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
