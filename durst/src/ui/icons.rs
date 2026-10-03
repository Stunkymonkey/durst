use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use std::io::{BufRead, Seek};

use ::image::imageops::{self, FilterType};
use ::image::{DynamicImage, ImageReader, ImageResult, Limits, RgbaImage};
use iced::widget::image::Handle;

use crate::core::notification::Notification;

#[derive(Debug, Clone)]
pub enum Icon {
    Svg(PathBuf),
    Raster(Handle),
}

/// Finds the icon of a notification in the order of the spec: `image-data`,
/// `image-path`, `app_icon`, then the icon of the `desktop-entry`. Paths may
/// be plain, `file://` URIs or icon names from the icon theme (`theme`, or
/// GTK's configured one).
pub fn resolve(n: &Notification, size: u32, theme: Option<&str>) -> Option<Icon> {
    if let Some(img) = &n.hints.image_data {
        let rgba = RgbaImage::from_raw(img.width, img.height, img.rgba.to_vec())?;
        return Some(Icon::Raster(scaled(rgba, size)));
    }
    let desktop_icon = n
        .hints
        .desktop_entry
        .as_deref()
        .and_then(desktop_entry_icon);
    [
        n.hints.image_path.as_deref(),
        Some(n.app_icon.as_str()),
        desktop_icon.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    .find_map(|s| lookup(s, size, theme))
    .and_then(|path| load(path, size))
}

/// Scales an image to fit `size` x `size`, keeping its aspect ratio.
///
/// The renderer must not scale raster images: iced_tiny_skia 0.14 positions
/// scaled images in image space and truncates to whole pixels there, which
/// moves upscaled images by up to the scale factor (a 2x2 icon drawn at 48px
/// lands 24px off). Scaling once here also avoids rescaling every frame.
pub(crate) fn scaled(img: RgbaImage, size: u32) -> Handle {
    let (w, h) = img.dimensions();
    let scale = size as f32 / w.max(h).max(1) as f32;
    let (sw, sh) = (
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
    );
    let img = if (sw, sh) == (w, h) {
        img
    } else {
        // keep tiny icons crisp instead of blurring them
        let filter = if scale >= 2.0 {
            FilterType::Nearest
        } else {
            FilterType::Triangle
        };
        imageops::resize(&img, sw, sh, filter)
    };
    Handle::from_rgba(sw, sh, img.into_raw())
}

/// Decodes an icon or cover with limits: a small file can describe a huge
/// image, and all are drawn at icon or cover size.
pub(crate) fn decode(reader: ImageReader<impl BufRead + Seek>) -> ImageResult<DynamicImage> {
    let mut reader = reader.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()
}

/// An icon by name or path, e.g. a rule's `default_icon`.
pub fn resolve_name(name: &str, size: u32, theme: Option<&str>) -> Option<Icon> {
    lookup(name, size, theme).and_then(|path| load(path, size))
}

fn load(path: PathBuf, size: u32) -> Option<Icon> {
    if path.extension().is_some_and(|e| e == "svg") {
        return Some(Icon::Svg(path));
    }
    match ImageReader::open(&path)
        .map_err(Into::into)
        .and_then(decode)
    {
        Ok(img) => Some(Icon::Raster(scaled(img.into_rgba8(), size))),
        Err(e) => {
            log::warn!("cannot load icon {}: {e}", path.display());
            None
        }
    }
}

fn lookup(name: &str, size: u32, theme: Option<&str>) -> Option<PathBuf> {
    let name = name.strip_prefix("file://").unwrap_or(name);
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    let theme = theme
        .or_else(|| gtk_theme().as_deref())
        .unwrap_or("hicolor");
    let size = u16::try_from(size).unwrap_or(u16::MAX);
    // falls back to the theme's parents, then hicolor
    let found = freedesktop_icons::lookup(name)
        .with_size(size)
        .with_theme(theme)
        .with_cache()
        .find();
    if found.is_none() {
        log::debug!("icon {name:?} not found in theme {theme:?}");
    }
    found
}

/// `gtk-icon-theme-name` from GTK's settings.ini, read once.
fn gtk_theme() -> &'static Option<String> {
    static THEME: OnceLock<Option<String>> = OnceLock::new();
    THEME.get_or_init(|| {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        ["gtk-4.0", "gtk-3.0"].into_iter().find_map(|dir| {
            let ini = std::fs::read_to_string(config.join(dir).join("settings.ini")).ok()?;
            ini.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "gtk-icon-theme-name").then(|| value.trim().to_owned())
            })
        })
    })
}

/// The `Icon=` of `<name>.desktop` in the XDG application directories.
fn desktop_entry_icon(name: &str) -> Option<String> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(dirs.split(':').map(PathBuf::from))
        .map(|dir| dir.join("applications").join(format!("{name}.desktop")))
        .find_map(|path| read_icon_key(&path))
}

fn read_icon_key(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry && let Some(icon) = line.strip_prefix("Icon=") {
            return Some(icon.trim().to_owned());
        }
    }
    None
}
