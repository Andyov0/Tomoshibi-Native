//! Watch together and music: how sharp a video watched together may get
//! here (Watching.cs `WatchCeilings`, `meet-live.watch-quality`), and how
//! music sounds in a call (duck.ts `meet-live.music-duck`, on unless "off";
//! preferences.ts `meet-live.music-quality`, "best" or nothing for
//! lossless). A call that is on reads them as it starts each thing.

use adw::prelude::*;

use super::new_page;
use crate::{
    app::{remote, AppModel, Change},
    i18n::t,
    ui::Context,
};

pub const WATCH_QUALITY_KEY: &str = "meet-live.watch-quality";
pub const MUSIC_QUALITY_KEY: &str = "meet-live.music-quality";
pub const MUSIC_DUCK_KEY: &str = "meet-live.music-duck";

/// The settings' own change, for a call that reads them live.
pub const CHANGED: Change = Change::Module("settings");

pub fn watching(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "watching",
        &t("Watch together"),
        "video-display-symbolic",
        &t("How sharp a video watched together may get here."),
    );
    let group = adw::PreferencesGroup::builder()
        .title(t("Watch together"))
        .description(t(
            "Where a video comes in several qualities, it is never fetched sharper than this here. It applies from the next video you open.",
        ))
        .build();
    // Resolutions are written the same in every language.
    let options = [("auto", t("Automatic")), ("1080p", "1080p".to_owned()), ("720p", "720p".to_owned())];
    let list = gtk::StringList::new(&options.iter().map(|(_, words)| words.as_str()).collect::<Vec<_>>());
    let combo = adw::ComboRow::builder()
        .title(t("Picture quality"))
        .model(&list)
        .build();
    combo.set_widget_name("settings.watch-quality");
    let kept = model.preferences().string(WATCH_QUALITY_KEY);
    combo.set_selected(
        options
            .iter()
            .position(|(value, _)| Some(*value) == kept.as_deref())
            .unwrap_or(0) as u32,
    );
    let weak = model.downgrade();
    combo.connect_selected_notify(move |combo| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        match combo.selected() {
            1 => model.preferences().set(WATCH_QUALITY_KEY, "1080p"),
            2 => model.preferences().set(WATCH_QUALITY_KEY, "720p"),
            _ => model.preferences().remove(WATCH_QUALITY_KEY),
        }
        model.announce(CHANGED);
    });
    group.add(&combo);
    group.set_sensitive(remote::allows(model, remote::feature::WATCH_TOGETHER) && model.holds("watch"));
    page.add(&group);
    page
}

pub fn music(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "music",
        &t("Music"),
        "emblem-music-symbolic",
        &t("How music sounds in a call."),
    );
    let best = model.preferences().string(MUSIC_QUALITY_KEY).as_deref() == Some("best");
    let group = adw::PreferencesGroup::builder().title(t("Music")).build();
    let footer = |model: &AppModel, best: bool| {
        if best && model.holds("music") {
            t("Hi-Res and masters are large, and take longer to start.")
        } else {
            String::new()
        }
    };
    group.set_description(Some(&footer(model, best)).filter(|said| !said.is_empty()).map(String::as_str));
    let duck = adw::SwitchRow::builder()
        .title(t("Lower the music while anybody talks"))
        .active(model.preferences().string(MUSIC_DUCK_KEY).as_deref() != Some("off"))
        .build();
    duck.set_widget_name("settings.music-duck");
    let weak = model.downgrade();
    duck.connect_active_notify(move |row| {
        if let Some(model) = weak.upgrade() {
            if row.is_active() {
                model.preferences().remove(MUSIC_DUCK_KEY);
            } else {
                model.preferences().set(MUSIC_DUCK_KEY, "off");
            }
            model.announce(CHANGED);
        }
    });
    group.add(&duck);
    if model.holds("music") {
        let list = gtk::StringList::new(&[t("Lossless").as_str(), t("Highest").as_str()]);
        let quality = adw::ComboRow::builder().title(t("Quality")).model(&list).build();
        quality.set_selected(if best { 1 } else { 0 });
        let (weak, group_weak) = (model.downgrade(), group.downgrade());
        quality.connect_selected_notify(move |row| {
            let Some(model) = weak.upgrade() else {
                return;
            };
            let best = row.selected() == 1;
            if best {
                model.preferences().set(MUSIC_QUALITY_KEY, "best");
            } else {
                model.preferences().remove(MUSIC_QUALITY_KEY);
            }
            if let Some(group) = group_weak.upgrade() {
                let said = footer(&model, best);
                group.set_description(Some(said.as_str()).filter(|said| !said.is_empty()));
            }
            model.announce(CHANGED);
        });
        group.add(&quality);
    }
    group.set_sensitive(remote::allows(model, remote::feature::MUSIC));
    page.add(&group);
    page
}
