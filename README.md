# durst

dunst written in Rust

## interfaces

### dbus-interface

- [org.freedesktop.Notifications](https://github.com/GNOME/gnome-shell/blob/main/data/dbus-interfaces/org.freedesktop.Notifications.xml): recieve notifications

### used wayland-protocols

- [ext-idle-notify-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-idle-notify/ext-idle-notify-v1.xml): detect user idle
- [ext-session-lock-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-session-lock/ext-session-lock-v1.xml): detect lockscreen
- [ext-foreign-toplevel-list-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml): check fullscreen applications

## ToDo

 - [x] `dbus`: get notifcations
 - [x] history: store notifications
 - [x] render
 - [x] multi window
 - [ ] rules: automate notifications
    - [ ] grid
    - [ ] style
    - [ ] history
 - [ ] replace dbus with zbus?
 - [ ] buttons?
