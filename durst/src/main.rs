mod cli;
mod config;
mod dbus_notifications;
mod gui;
mod icons;
mod notification;
mod test;

use dbus::arg;
use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::blocking::LocalConnection;
use dbus_tree as tree;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use std::env::var;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use crate::gui::Flags;
use crate::gui::UINotification;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::MultiApplication;

use config::Config;
use notification::RawNotification;

#[derive(Debug)]
struct Container {
    queue: Vec<RawNotification>,
    config: Vec<Config>,
}

type Err = tree::MethodErr;

impl dbus_notifications::OrgFreedesktopNotifications for Mutex<Container> {
    fn get_capabilities(&self) -> Result<Vec<String>, Err> {
        debug!("get_capabilities");
        Ok(vec!["test".to_string()])
    }
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<&str>,
        hints: ::std::collections::HashMap<String, arg::Variant<Box<dyn arg::RefArg>>>,
        expire_timeout: i32,
    ) -> Result<u32, Err> {
        let new_notification = RawNotification::new(
            app_name,
            replaces_id,
            app_icon,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        );
        debug!("notify {:?}", new_notification);
        let mut data = self.lock().unwrap();
        (*data).queue.push(new_notification.clone());

        // display the notification
        let flags = Flags {
            notification: new_notification,
            app_icon: icons::get_icon(app_icon),
        };
        let _ = UINotification::run(Settings {
            layer_settings: LayerShellSettings {
                size: Some((400, 100)),
                exclusive_zone: 0,
                margin: (50, 50, 50, 50),
                anchor: Anchor::Right | Anchor::Top,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            },
            flags,
            ..Default::default()
        });

        Ok((*data).queue.len() as u32)
    }
    fn close_notification(&self, id: u32) -> Result<(), Err> {
        debug!("close_notification {:?}", id);
        Ok(())
    }
    fn get_server_information(&self) -> Result<(String, String, String, String), Err> {
        debug!("getserverinformation");
        Ok((
            env!("CARGO_PKG_NAME").to_string(),
            "durst-notification.org".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        ))
    }
}

impl AsRef<dyn dbus_notifications::OrgFreedesktopNotifications + 'static> for Rc<Mutex<Container>> {
    fn as_ref(&self) -> &(dyn dbus_notifications::OrgFreedesktopNotifications + 'static) {
        &**self
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_home = var("XDG_CONFIG_HOME")
        .or_else(|_| var("HOME").map(|home| format!("{}/.config", home)))
        .unwrap();
    let tmp = config::load_config(format!("{}/durst/config.yml", config_home));

    let container_rc = Rc::new(Mutex::new(Container {
        queue: Vec::<RawNotification>::new(),
        config: tmp,
    }));

    let factory = tree::Factory::new_fn::<()>();
    let iface = dbus_notifications::org_freedesktop_notifications_server(&factory, (), move |_| {
        Rc::clone(&container_rc)
    });

    let c = LocalConnection::new_session()?;

    let r = c.request_name("org.freedesktop.Notifications", false, true, true)?;
    if r != RequestNameReply::PrimaryOwner {
        panic!("Another notification daemon is running!");
    }

    let tree = factory
        .tree(())
        // needed for introspectable of children
        .add(factory.object_path("/", ()).introspectable())
        .add(
            factory
                .object_path("/org/freedesktop/Notifications", ())
                .introspectable()
                .add(iface),
        );
    tree.start_receive(&c);

    loop {
        c.process(Duration::from_millis(1000))?;
    }
}

fn main() {
    env_logger::init();

    let matches = cli::build_cli().get_matches();
    if let Some(mode) = matches.get_one::<String>("mode") {
        println!("Mode: {}", mode);
    } else {
        println!("No mode provided");
    }

    if let Err(e) = run() {
        println!("{}", e);
    }
}
