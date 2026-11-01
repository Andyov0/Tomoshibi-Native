//! General: the language, the accent colour, the connection badge and where
//! a call's controls sit (Settings.swift `GeneralSettings`), each kept under
//! the web's or the other apps' own name.

use adw::prelude::*;
use gtk::glib;

use super::new_page;
use crate::{
    i18n::{self, t, Language},
    theme::{self, Accent, ACCENT_KEY, BADGE_KEY, CONTROLS_KEY},
    ui::{kit, Context},
};

pub fn page(context: &Context) -> adw::PreferencesPage {
    let model = &context.model;
    let page = new_page(
        "general",
        &t("General"),
        "emblem-system-symbolic",
        &t("Language, colour, and how a call is laid out."),
    );

    // ---- the language ---------------------------------------------------

    let language = adw::PreferencesGroup::builder()
        .title(t("Language"))
        .build();
    let system = Language::detect(i18n::system_languages().iter().map(String::as_str));
    let mut names = vec![format!(
        "{} ({})",
        t("Same as the system"),
        system.display_name()
    )];
    names.extend(
        Language::ALL
            .iter()
            .map(|language| language.display_name().to_owned()),
    );
    let list = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let combo = adw::ComboRow::builder()
        .title(t("Language"))
        .model(&list)
        .build();
    combo.set_widget_name("settings.language");
    let chosen = if i18n::chosen() {
        Language::ALL
            .iter()
            .position(|language| *language == i18n::language())
            .map(|index| index as u32 + 1)
            .unwrap_or(0)
    } else {
        0
    };
    combo.set_selected(chosen);
    let weak = model.downgrade();
    combo.connect_selected_notify(move |combo| {
        let selected = combo.selected() as usize;
        let choice = selected
            .checked_sub(1)
            .and_then(|index| Language::ALL.get(index).copied());
        // Changed after this row's own signal has returned: the language
        // change words the dialog again, this row with it.
        let weak = weak.clone();
        glib::idle_add_local_once(move || {
            if let Some(model) = weak.upgrade() {
                match choice {
                    Some(language) => model.preferences().set(i18n::STORAGE_KEY, language.code()),
                    None => model.preferences().remove(i18n::STORAGE_KEY),
                }
            }
            i18n::choose(choice);
        });
    });
    language.add(&combo);
    page.add(&language);

    // ---- the accent -------------------------------------------------------

    let accent = adw::PreferencesGroup::builder()
        .title(t("Accent colour"))
        .build();
    let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    swatches.set_margin_top(12);
    swatches.set_margin_bottom(12);
    swatches.set_margin_start(12);
    swatches.set_margin_end(12);
    swatches.set_halign(gtk::Align::Start);
    swatches.set_widget_name("settings.accent");
    let sample = kit::primary_button(&t("Join"));
    sample.set_halign(gtk::Align::End);
    sample.set_valign(gtk::Align::Center);
    sample.set_width_request(120);
    sample.set_can_target(false);
    let accent_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    swatches.set_hexpand(true);
    accent_row.append(&swatches);
    accent_row.append(&sample);
    accent_row.set_margin_end(12);
    let accent_holder = gtk::ListBox::new();
    accent_holder.add_css_class("boxed-list");
    accent_holder.set_selection_mode(gtk::SelectionMode::None);
    accent_holder.append(&accent_row);
    for one in Accent::ALL {
        let swatch = swatch(one);
        let weak = model.downgrade();
        let swatches_weak = swatches.downgrade();
        swatch.connect_clicked(move |_| {
            theme::set_accent(one);
            if let Some(model) = weak.upgrade() {
                model.preferences().set(ACCENT_KEY, one.key());
            }
            if let Some(swatches) = swatches_weak.upgrade() {
                let mut child = swatches.first_child();
                while let Some(next) = child {
                    next.queue_draw();
                    child = next.next_sibling();
                }
            }
        });
        swatches.append(&swatch);
    }
    accent.add(&accent_holder);
    page.add(&accent);

    // ---- the call's furniture ------------------------------------------------

    let call = adw::PreferencesGroup::builder()
        .title(t("Controls"))
        .build();
    let badge = choice_row(
        model,
        &t("Connection badge"),
        BADGE_KEY,
        &[
            ("signal", t("Signal only")),
            ("detailed", t("With details")),
            ("hidden", t("Hidden")),
        ],
    );
    badge.set_widget_name("settings.badge");
    let controls = choice_row(
        model,
        &t("Controls"),
        CONTROLS_KEY,
        &[
            ("always", t("Always shown")),
            ("idle", t("Hide when nothing is happening")),
            ("side", t("At the side")),
        ],
    );
    call.add(&badge);
    call.add(&controls);
    page.add(&call);
    page
}

/// A choice among values kept under `key`, the first being the default
/// (kept as nothing, as the web keeps a default).
fn choice_row(
    model: &crate::app::AppModel,
    title: &str,
    key: &'static str,
    options: &[(&'static str, String)],
) -> adw::ComboRow {
    let list = gtk::StringList::new(
        &options
            .iter()
            .map(|(_, words)| words.as_str())
            .collect::<Vec<_>>(),
    );
    let combo = adw::ComboRow::builder().title(title).model(&list).build();
    let kept = model.preferences().string(key);
    let selected = options
        .iter()
        .position(|(value, _)| Some(*value) == kept.as_deref())
        .unwrap_or(0);
    combo.set_selected(selected as u32);
    let values: Vec<&'static str> = options.iter().map(|(value, _)| *value).collect();
    let weak = model.downgrade();
    combo.connect_selected_notify(move |combo| {
        let Some(model) = weak.upgrade() else {
            return;
        };
        match values.get(combo.selected() as usize) {
            Some(value) if combo.selected() > 0 => model.preferences().set(key, *value),
            _ => model.preferences().remove(key),
        }
        model.announce(crate::app::Change::Module("settings"));
    });
    combo
}

/// One colour of the accent, ringed where it is the one chosen.
fn swatch(accent: Accent) -> gtk::Button {
    let area = gtk::DrawingArea::builder()
        .content_width(28)
        .content_height(28)
        .can_target(false)
        .build();
    area.set_draw_func(move |_, cr, width, height| {
        let size = width.min(height) as f64;
        let colour = accent.color(theme::system_scheme());
        let chosen = theme::accent() == accent;
        let radius = if chosen {
            size / 2.0 - 5.0
        } else {
            size / 2.0 - 1.0
        };
        cr.arc(size / 2.0, size / 2.0, radius, 0.0, std::f64::consts::TAU);
        cr.set_source_rgb(colour.red as f64, colour.green as f64, colour.blue as f64);
        let _ = cr.fill();
        if chosen {
            cr.arc(
                size / 2.0,
                size / 2.0,
                size / 2.0 - 1.5,
                0.0,
                std::f64::consts::TAU,
            );
            cr.set_line_width(2.0);
            let _ = cr.stroke();
        }
    });
    let button = gtk::Button::builder().child(&area).build();
    button.add_css_class("tmsb-swatch");
    button.set_tooltip_text(Some(accent.key()));
    button.update_property(&[gtk::accessible::Property::Label(accent.key())]);
    button
}
