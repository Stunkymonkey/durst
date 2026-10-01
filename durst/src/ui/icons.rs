use std::path::PathBuf;

use crate::core::notification::Notification;

/// Finds the icon file for a notification: the `image-path` hint first, then
/// `app_icon`. Both may be a file path, a `file://` URI or an icon name.
pub fn resolve(n: &Notification, size: u32) -> Option<PathBuf> {
    [n.hints.image_path.as_deref(), Some(n.app_icon.as_str())]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .find_map(|s| lookup(s, size))
}

fn lookup(name: &str, size: u32) -> Option<PathBuf> {
    let name = name.strip_prefix("file://").unwrap_or(name);
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let size = u16::try_from(size).unwrap_or(u16::MAX);
    match linicon::lookup_icon(name)
        .with_size(size)
        .use_fallback_themes(true)
        .next()
    {
        Some(Ok(icon)) => Some(icon.path),
        Some(Err(e)) => {
            log::debug!("icon lookup for {name:?} failed: {e:?}");
            None
        }
        None => {
            log::debug!("icon {name:?} not found");
            None
        }
    }
}
