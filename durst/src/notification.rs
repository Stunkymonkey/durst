use dbus::arg;
use dbus::arg::{RefArg, Variant};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RawNotification {
    pub id: u32,
    pub app_name: String,
    pub replaces_id: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    // pub hints: HashMap<String, arg::Variant<Box<dyn arg::RefArg>>>,
    pub hints: HashMap<String, arg::Variant<Arc<dyn arg::RefArg>>>,
    pub expire_timeout: String,
}

static ID_COUNTER: AtomicU32 = AtomicU32::new(1);

impl RawNotification {
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

        let new_hints = hints
            .into_iter()
            .map(|(key, value)| {
                // Access the inner `Box<dyn RefArg>` using `Variant.0` and wrap it in `Arc`
                let arc_value = Variant(Arc::from(value.0));
                (key, arc_value)
            })
            .collect();

        RawNotification {
            id: id,
            app_name: app_name.to_string(),
            replaces_id: replaces_id.to_string(),
            app_icon: app_icon.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions: actions_vec,
            hints: new_hints,
            expire_timeout: expire_timeout.to_string(),
        }
    }

    pub fn default() -> Self {
        RawNotification::new(
            "DefaultAppName",
            0,
            "DefaultAppIcon",
            "DefaultSummary",
            "DefaultBody",
            vec![],
            HashMap::new(),
            5000,
        )
    }
}
