//! Describes the settings form once, so the UI, parsing and the "effective
//! settings" view all stay in sync with `valo_proto::Settings`.

use serde_json::{Map, Value};
use valo_proto::Settings;

pub enum Kind {
    Choice(&'static [(&'static str, &'static str)]),
    Text { placeholder: &'static str },
    /// One value per line (textarea).
    Lines { placeholder: &'static str },
    Bool,
    Apps,
}

pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

pub struct Section {
    pub title: &'static str,
    pub icon: &'static str,
    pub fields: &'static [Field],
}

pub const KEYBOARDS: &[(&str, &str)] = &[
    ("ch", "Schweiz (Deutsch)"),
    ("ch:fr", "Schweiz (Französisch)"),
    ("de", "Deutschland"),
    ("fr", "Frankreich"),
    ("it", "Italien"),
    ("at", "Österreich"),
    ("us", "USA (Englisch)"),
    ("gb", "Grossbritannien"),
];

pub const LOCALES: &[(&str, &str)] = &[
    ("de_CH.UTF-8", "Deutsch (Schweiz)"),
    ("fr_CH.UTF-8", "Französisch (Schweiz)"),
    ("it_CH.UTF-8", "Italienisch (Schweiz)"),
    ("de_DE.UTF-8", "Deutsch (Deutschland)"),
    ("de_AT.UTF-8", "Deutsch (Österreich)"),
    ("en_GB.UTF-8", "Englisch (GB)"),
    ("en_US.UTF-8", "Englisch (USA)"),
];

pub const TIMEZONES: &[(&str, &str)] = &[
    ("Europe/Zurich", "Zürich"),
    ("Europe/Berlin", "Berlin"),
    ("Europe/Vienna", "Wien"),
    ("Europe/Rome", "Rom"),
    ("Europe/Paris", "Paris"),
    ("Europe/London", "London"),
    ("UTC", "UTC"),
];

pub static SECTIONS: &[Section] = &[
    Section {
        title: "Sprache & Tastatur",
        icon: "⌨",
        fields: &[
            Field { key: "keyboard", label: "Tastaturlayout", help: "", kind: Kind::Choice(KEYBOARDS) },
            Field { key: "locale", label: "Sprache", help: "Sprache der Oberfläche und Formate", kind: Kind::Choice(LOCALES) },
            Field { key: "timezone", label: "Zeitzone", help: "", kind: Kind::Choice(TIMEZONES) },
        ],
    },
    Section {
        title: "Benutzerdaten",
        icon: "🗂",
        fields: &[Field {
            key: "home_mode",
            label: "Persönlicher Ordner beim Abmelden",
            help: "Löschen = jedes Mal ein sauberes Gerät. Dateien gehören in die Cloud.",
            kind: Kind::Choice(&[("wipe", "Löschen"), ("keep", "Behalten")]),
        }],
    },
    Section {
        title: "Browser",
        icon: "🌐",
        fields: &[
            Field { key: "browser_homepage", label: "Startseite", help: "", kind: Kind::Text { placeholder: "https://…" } },
            Field {
                key: "browser_search",
                label: "Suchmaschine",
                help: "",
                kind: Kind::Choice(&[
                    ("ecosia", "Ecosia"),
                    ("duckduckgo", "DuckDuckGo"),
                    ("startpage", "Startpage"),
                    ("google", "Google"),
                ]),
            },
            Field { key: "browser_adblock", label: "Werbeblocker (uBlock Origin)", help: "", kind: Kind::Bool },
            Field {
                key: "browser_bookmarks",
                label: "Lesezeichen",
                help: "Eine Zeile pro Lesezeichen: Name | Adresse. Erscheinen fest in der Lesezeichenleiste.",
                kind: Kind::Lines { placeholder: "Schulwebsite | https://www.schule.ch\nLernplattform | https://…" },
            },
            Field {
                key: "browser_filter",
                label: "Webfilter",
                help: "Sperrliste: die Seiten unten sind gesperrt. Nur erlaubte Seiten: nur die Seiten unten plus Startseite und Suchmaschine.",
                kind: Kind::Choice(&[("off", "Aus"), ("block", "Sperrliste"), ("allow", "Nur erlaubte Seiten")]),
            },
            Field {
                key: "browser_sites",
                label: "Seiten für den Webfilter",
                help: "Eine Domain pro Zeile, Unterseiten sind eingeschlossen (z. B. tiktok.com).",
                kind: Kind::Lines { placeholder: "tiktok.com\nbeispiel.ch" },
            },
            Field {
                key: "browser_allow_addons",
                label: "Eigene Erweiterungen erlaubt",
                help: "Benutzer dürfen selbst Firefox-Erweiterungen installieren",
                kind: Kind::Bool,
            },
        ],
    },
    Section {
        title: "Apps",
        icon: "▦",
        fields: &[Field {
            key: "apps",
            label: "Installierte Apps",
            help: "Nur Apps aus dem Katalog. Firefox und ArtCraft sind immer im Image.",
            kind: Kind::Apps,
        }],
    },
    Section {
        title: "Rechte",
        icon: "🔑",
        fields: &[
            Field {
                key: "allow_app_install",
                label: "Apps selbst installieren",
                help: "Benutzer dürfen Apps aus dem Katalog selbst installieren",
                kind: Kind::Bool,
            },
            Field {
                key: "allow_user_settings",
                label: "Persönliche Einstellungen ändern",
                help: "z. B. Hintergrundbild, Tastatur",
                kind: Kind::Bool,
            },
            Field { key: "allow_lock_screen", label: "Bildschirm sperren erlaubt", help: "Super+L", kind: Kind::Bool },
        ],
    },
    Section {
        title: "Updates",
        icon: "⟳",
        fields: &[Field {
            key: "update_ring",
            label: "Update-Ring",
            help: "Neue Images zuerst auf Testgeräten, dann Pilot, dann alle",
            kind: Kind::Choice(&[("test", "Test"), ("pilot", "Pilot"), ("all", "Alle")]),
        }],
    },
];

pub fn to_map(s: &Settings) -> Map<String, Value> {
    match serde_json::to_value(s).expect("settings serialize") {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Human readable value of a field, e.g. "Schweiz (Deutsch)" or "Ja".
pub fn display(field: &Field, v: Option<&Value>, catalog: &[crate::db::CatalogEntry]) -> String {
    let Some(v) = v else { return "–".into() };
    match (&field.kind, v) {
        (Kind::Choice(opts), Value::String(s)) => {
            opts.iter().find(|(k, _)| k == s).map(|(_, l)| l.to_string()).unwrap_or_else(|| s.clone())
        }
        (Kind::Bool, Value::Bool(b)) => if *b { "Ja" } else { "Nein" }.into(),
        (Kind::Apps, Value::Array(a)) if a.is_empty() => "keine zusätzlichen".into(),
        (Kind::Lines { .. }, Value::Array(a)) if a.is_empty() => "keine".into(),
        (Kind::Lines { .. }, Value::Array(a)) => {
            let first: Vec<&str> = a.iter().filter_map(|x| x.as_str()).take(3).collect();
            let more = if a.len() > 3 { format!(" (+{})", a.len() - 3) } else { String::new() };
            format!("{}{more}", first.join(", "))
        }
        (Kind::Apps, Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str())
            .map(|id| catalog.iter().find(|c| c.app.id == id).map(|c| c.app.name.clone()).unwrap_or(id.into()))
            .collect::<Vec<_>>()
            .join(", "),
        (_, Value::String(s)) => s.clone(),
        (_, other) => other.to_string(),
    }
}

/// Parse the settings form. Empty value / "inherit" means "not set at this level".
pub fn parse_form(form: &crate::util::FormData, catalog: &[crate::db::CatalogEntry]) -> Settings {
    let mut m = Map::new();
    for f in SECTIONS.iter().flat_map(|s| s.fields.iter()) {
        let v = form.text(f.key);
        let value = match &f.kind {
            Kind::Choice(opts) => opts.iter().any(|(k, _)| *k == v).then(|| Value::String(v.clone())),
            Kind::Text { .. } => (!v.is_empty()).then(|| Value::String(v.chars().take(500).collect())),
            Kind::Lines { .. } => {
                let lines: Vec<Value> = v
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .take(200)
                    .map(|l| Value::String(l.chars().take(300).collect()))
                    .collect();
                (!lines.is_empty()).then_some(Value::Array(lines))
            }
            Kind::Bool => match v.as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            Kind::Apps => (form.text("apps_mode") == "set").then(|| {
                Value::Array(
                    form.all("apps_item")
                        .into_iter()
                        .filter(|id| catalog.iter().any(|c| &c.app.id == id))
                        .map(Value::String)
                        .collect(),
                )
            }),
        };
        if let Some(value) = value {
            m.insert(f.key.to_string(), value);
        }
    }
    serde_json::from_value(Value::Object(m)).unwrap_or_default()
}
