//! Side effects of notifications: scripts, sounds and opening links. All of
//! them spawn a process and never block the UI loop.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Sound;
use crate::core::notification::Notification;
use crate::core::rules::ScriptOn;

/// Starts `command` with `args`; the child is reaped on a thread so it
/// doesn't stay a zombie.
pub fn spawn(command: &[String], extra_args: &[&str], env: &[(&str, String)]) {
    let Some((program, args)) = command.split_first() else {
        return;
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .args(extra_args)
        .envs(env.iter().map(|(k, v)| (k, v)));
    match cmd.spawn() {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => log::warn!("cannot run {program}: {e}"),
    }
}

/// Runs a rule's script with the notification in `DURST_*` variables.
/// `detail` is the invoked action key, or the close reason.
pub fn run_script(command: &[String], n: &Notification, on: ScriptOn, detail: Option<&str>) {
    let h = &n.hints;
    let opt = |s: &Option<String>| s.clone().unwrap_or_default();
    let mut env = vec![
        ("DURST_ID", n.id.to_string()),
        ("DURST_APP_NAME", n.app_name.clone()),
        ("DURST_SUMMARY", n.summary.clone()),
        ("DURST_BODY", n.body.clone()),
        ("DURST_ICON", n.app_icon.clone()),
        ("DURST_URGENCY", h.urgency.name().to_owned()),
        ("DURST_CATEGORY", opt(&h.category)),
        ("DURST_DESKTOP_ENTRY", opt(&h.desktop_entry)),
        ("DURST_STACK_TAG", opt(&h.stack_tag)),
    ];
    match on {
        ScriptOn::Receive => env.push(("DURST_EVENT", "receive".into())),
        ScriptOn::Action => {
            env.push(("DURST_EVENT", "action".into()));
            env.push(("DURST_ACTION", detail.unwrap_or_default().into()));
        }
        ScriptOn::Close => {
            env.push(("DURST_EVENT", "close".into()));
            env.push(("DURST_CLOSE_REASON", detail.unwrap_or_default().into()));
        }
    }
    log::debug!("script {command:?} for {} on {on:?}", n.id);
    spawn(command, &[], &env);
}

/// Plays a sound file or a sound theme name.
pub fn play_sound(config: &Sound, sound: &str) {
    let path = if sound.starts_with('/') {
        Some(PathBuf::from(sound))
    } else {
        find_sound(sound, &config.theme, &data_dirs())
    };
    match path {
        Some(path) => {
            log::debug!("play {}", path.display());
            spawn(&config.command, &[&path.to_string_lossy()], &[]);
        }
        None => log::debug!("sound {sound:?} not found in theme {:?}", config.theme),
    }
}

fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from))
        .collect()
}

/// Looks up a sound by name as in the freedesktop sound theme spec: the
/// theme's `stereo` profile, then its root, then the `freedesktop` theme; a
/// name like `message-new-instant` falls back to `message-new`, `message`.
fn find_sound(name: &str, theme: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut name = name;
    loop {
        for theme in [theme, "freedesktop"] {
            for dir in dirs {
                let base = dir.join("sounds").join(theme);
                for sub in [base.join("stereo"), base.clone()] {
                    if let Some(found) = with_extension(&sub, name) {
                        return Some(found);
                    }
                }
            }
        }
        name = &name[..name.rfind('-')?];
    }
}

fn with_extension(dir: &Path, name: &str) -> Option<PathBuf> {
    ["oga", "ogg", "wav"]
        .iter()
        .map(|ext| dir.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_theme_lookup() {
        let dir = std::env::temp_dir().join(format!("durst-sound-test-{}", std::process::id()));
        let stereo = dir.join("sounds/freedesktop/stereo");
        std::fs::create_dir_all(&stereo).unwrap();
        std::fs::write(stereo.join("message.oga"), b"").unwrap();
        std::fs::create_dir_all(dir.join("sounds/mytheme")).unwrap();
        std::fs::write(dir.join("sounds/mytheme/bell.wav"), b"").unwrap();
        let dirs = [dir.clone()];

        assert_eq!(
            find_sound("message-new-instant", "mytheme", &dirs),
            Some(stereo.join("message.oga"))
        );
        assert_eq!(
            find_sound("bell", "mytheme", &dirs),
            Some(dir.join("sounds/mytheme/bell.wav"))
        );
        assert_eq!(find_sound("nothing", "mytheme", &dirs), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
