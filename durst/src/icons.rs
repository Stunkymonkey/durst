use linicon;
use std::path::PathBuf;

pub fn get_icon(name: &str) -> PathBuf {
    // Attempt to lookup the icon using linicon
    match linicon::lookup_icon(name).use_fallback_themes(true).next() {
        Some(Ok(icon)) => icon.path,
        Some(Err(e)) => {
            eprintln!("Error finding icon: {:?}", e); // Log the error
            PathBuf::new() // Fallback to empty path
        }
        None => {
            eprintln!("Icon not found: {}", name); // Log the absence of the icon
            PathBuf::new() // Fallback to empty path
        }
    }
}
