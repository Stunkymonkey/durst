//! Cover art of media players: `file://` or `http(s)://` URLs, loaded off
//! the UI loop and scaled to their display size once.

use std::time::Duration;

use iced::widget::image::Handle;

const TIMEOUT: Duration = Duration::from_secs(5);
/// covers are small; anything bigger is not a cover
const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Loads and scales a cover; `None` if it can't be had.
pub async fn load(url: String, size: u32) -> Option<Handle> {
    let result = tokio::task::spawn_blocking(move || {
        let bytes = fetch(&url)?;
        let img = super::icons::decode(::image::ImageReader::new(std::io::Cursor::new(&bytes)))
            .map_err(|e| format!("{url}: {e}"))?;
        Ok::<_, String>(super::icons::scaled(img.into_rgba8(), size))
    })
    .await;
    match result {
        Ok(Ok(handle)) => Some(handle),
        Ok(Err(e)) => {
            log::warn!("cover art: {e}");
            None
        }
        Err(e) => {
            log::warn!("cover art: {e}");
            None
        }
    }
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    if let Some(path) = url.strip_prefix("file://") {
        let path = percent_decode(path);
        return std::fs::read(&path).map_err(|e| format!("{path}: {e}"));
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .new_agent();
        let mut response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
        return response
            .body_mut()
            .with_config()
            .limit(MAX_BYTES)
            .read_to_vec()
            .map_err(|e| format!("{url}: {e}"));
    }
    Err(format!("unsupported cover URL {url}"))
}

/// `%20` and friends in file URLs.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_file_urls() {
        assert_eq!(
            percent_decode("/tmp/my%20cover%C3%A4.png"),
            "/tmp/my coverä.png"
        );
        assert_eq!(percent_decode("/a%2"), "/a%2");
        assert_eq!(percent_decode("/plain"), "/plain");
    }

    #[test]
    fn rejects_other_schemes() {
        assert!(fetch("ftp://x/y.png").is_err());
    }
}
