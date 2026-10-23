//! Media players (MPRIS) and which one the media OSD and `durstctl media`
//! refer to: the one that most recently started playing, otherwise the one
//! active last.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl Status {
    pub fn parse(s: &str) -> Self {
        match s {
            "Playing" => Status::Playing,
            "Paused" => Status::Paused,
            _ => Status::Stopped,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Playing => "playing",
            Status::Paused => "paused",
            Status::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
}

impl Track {
    /// Players send metadata repeatedly (e.g. first without, then with the
    /// cover), so only these fields make another track.
    fn same_song(&self, other: &Track) -> bool {
        (&self.id, &self.title, &self.artist) == (&other.id, &other.title, &other.artist)
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    /// the bus name, `org.mpris.MediaPlayer2.<name>`
    pub name: String,
    /// e.g. "Spotify"
    pub identity: String,
    pub status: Status,
    pub track: Track,
    /// when it last started playing or changed its track while playing
    active_at: u64,
}

#[derive(Debug, Default)]
pub struct Players {
    players: Vec<Player>,
    clock: u64,
}

impl Players {
    /// Applies what a player reported; `true` if the active player now plays
    /// another song than before (the media OSD shows it then).
    pub fn update(
        &mut self,
        name: &str,
        identity: Option<String>,
        status: Option<Status>,
        track: Option<Track>,
    ) -> bool {
        let before = self.active().map(|p| (p.name.clone(), p.track.clone()));
        self.clock += 1;
        let clock = self.clock;
        let player = match self.players.iter_mut().position(|p| p.name == name) {
            Some(i) => &mut self.players[i],
            None => {
                self.players.push(Player {
                    name: name.to_owned(),
                    identity: name
                        .strip_prefix("org.mpris.MediaPlayer2.")
                        .unwrap_or(name)
                        .to_owned(),
                    status: Status::Stopped,
                    track: Track::default(),
                    active_at: clock,
                });
                self.players.last_mut().unwrap()
            }
        };
        if let Some(identity) = identity {
            player.identity = identity;
        }
        if let Some(status) = status {
            if status == Status::Playing && player.status != Status::Playing {
                player.active_at = clock;
            }
            player.status = status;
        }
        if let Some(track) = track {
            if player.status == Status::Playing && !player.track.same_song(&track) {
                player.active_at = clock;
            }
            player.track = track;
        }
        match (before, self.active()) {
            (_, None) => false,
            (None, Some(now)) => !now.track.title.is_empty(),
            (Some((name, track)), Some(now)) => {
                !now.track.title.is_empty() && (name != now.name || !track.same_song(&now.track))
            }
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.players.retain(|p| p.name != name);
    }

    pub fn active(&self) -> Option<&Player> {
        let latest = |playing: bool| {
            self.players
                .iter()
                .filter(|p| !playing || p.status == Status::Playing)
                .max_by_key(|p| p.active_at)
        };
        latest(true).or_else(|| latest(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str) -> Track {
        Track {
            id: title.into(),
            title: title.into(),
            artist: "artist".into(),
            ..Track::default()
        }
    }

    #[test]
    fn track_changes_show_the_osd() {
        let mut p = Players::default();
        assert!(p.update("a", None, Some(Status::Playing), Some(track("one"))));
        // the same song again, now with a cover: no new OSD
        let mut with_art = track("one");
        with_art.art_url = Some("file:///cover.png".into());
        assert!(!p.update("a", None, None, Some(with_art)));
        assert_eq!(
            p.active().unwrap().track.art_url.as_deref(),
            Some("file:///cover.png")
        );
        assert!(!p.update("a", None, Some(Status::Paused), None));
        assert!(p.update("a", None, None, Some(track("two"))));
    }

    #[test]
    fn the_latest_playing_player_is_active() {
        let mut p = Players::default();
        p.update(
            "a",
            Some("A".into()),
            Some(Status::Playing),
            Some(track("one")),
        );
        p.update(
            "b",
            Some("B".into()),
            Some(Status::Paused),
            Some(track("two")),
        );
        assert_eq!(p.active().unwrap().identity, "A", "b is not playing");
        assert!(
            p.update("b", None, Some(Status::Playing), None),
            "b took over"
        );
        assert_eq!(p.active().unwrap().identity, "B");
        // b pauses: a is the one playing
        p.update("b", None, Some(Status::Paused), None);
        assert_eq!(p.active().unwrap().identity, "A");
        // nothing plays: the last active one
        p.update("a", None, Some(Status::Stopped), None);
        assert_eq!(p.active().unwrap().identity, "B");
        p.remove("org.mpris.MediaPlayer2.x");
        p.remove("b");
        assert_eq!(p.active().unwrap().identity, "A");
    }

    #[test]
    fn identity_defaults_to_the_bus_name() {
        let mut p = Players::default();
        p.update("org.mpris.MediaPlayer2.mpv", None, None, None);
        assert_eq!(p.active().unwrap().identity, "mpv");
    }
}
