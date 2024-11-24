use dbus::arg;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub replaces_id: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub hints: HashMap<String, arg::Variant<Box<dyn arg::RefArg>>>,
    pub expire_timeout: String,
}

static ID_COUNTER: AtomicU32 = AtomicU32::new(1);

impl Notification {
    pub fn new(
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<&str>,
        hints: HashMap<String, arg::Variant<Box<dyn arg::RefArg>>>,
        expire_timeout: i32,
    ) -> Self {
        let id;
        if replaces_id == 0 {
            id = ID_COUNTER.fetch_add(1, Ordering::Relaxed)
        } else {
            // TODO: Ensure the replacement id is valid
            id = replaces_id;
        }

        let actions_vec: Vec<String> = actions.iter().map(|s| s.to_string()).collect();
        let hints_map: HashMap<String, arg::Variant<Box<dyn arg::RefArg>>> =
            hints.into_iter()
                .map(|(s, hint)| (s.to_string(), hint))
                .collect();

        Notification {
            id: id,
            app_name: app_name.to_string(),
            replaces_id: replaces_id.to_string(),
            app_icon: app_icon.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions: actions_vec,
            hints: hints_map,
            expire_timeout: expire_timeout.to_string(),
        }
    }
}
