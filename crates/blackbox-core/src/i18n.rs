//! The few strings the native side renders itself (overlay notifications, tray tooltip).
//! Everything else is translated by the UI.

use crate::kb::Kb;
use crate::model::{Incident, IncidentKind, Lang, Unit};

pub fn kind_label(kind: IncidentKind, lang: Lang) -> &'static str {
    match (kind, lang) {
        (IncidentKind::Hitch, Lang::En) => "Stutter",
        (IncidentKind::Hitch, Lang::Fr) => "Saccade",
        (IncidentKind::Freeze, Lang::En) => "Freeze",
        (IncidentKind::Freeze, Lang::Fr) => "Gel de l'image",
        (IncidentKind::Latency, Lang::En) => "Driver latency spike",
        (IncidentKind::Latency, Lang::Fr) => "Pic de latence pilote",
        (IncidentKind::Hang, Lang::En) => "Not responding",
        (IncidentKind::Hang, Lang::Fr) => "Ne répond plus",
        (IncidentKind::Throttle, Lang::En) => "Throttling",
        (IncidentKind::Throttle, Lang::Fr) => "Bridage",
        (IncidentKind::Pressure, Lang::En) => "System pressure",
        (IncidentKind::Pressure, Lang::Fr) => "Système saturé",
    }
}

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Ms => "ms",
        Unit::Us => "µs",
        Unit::S => "s",
        Unit::Pct => "%",
    }
}

/// Title and body for a native notification about `incident`.
pub fn incident_notification(kb: &Kb, incident: &Incident, lang: Lang) -> (String, String) {
    let value = incident.magnitude.value.round();
    let mut title = format!("{} · {} {}", kind_label(incident.kind, lang), value, unit_label(incident.magnitude.unit));
    if let Some(app) = &incident.app {
        title.push_str(" · ");
        title.push_str(&app.name);
    }
    let body = incident
        .causes
        .first()
        .map(|c| kb.title(&c.id, lang, &c.params))
        .unwrap_or_else(|| match lang {
            Lang::En => "Open Blackbox for details".to_owned(),
            Lang::Fr => "Ouvre Blackbox pour les détails".to_owned(),
        });
    (title, body)
}

pub fn session_summary(app: &str, hitches: u32, minutes: u64, lang: Lang) -> (String, String) {
    match lang {
        Lang::En => (
            format!("Session ended · {app}"),
            format!("{hitches} stutter{} in {minutes} min. Open Blackbox to see why.", if hitches == 1 { "" } else { "s" }),
        ),
        Lang::Fr => (
            format!("Session terminée · {app}"),
            format!("{hitches} saccade{} en {minutes} min. Ouvre Blackbox pour voir pourquoi.", if hitches > 1 { "s" } else { "" }),
        ),
    }
}

pub fn test_notification(lang: Lang) -> (String, String) {
    match lang {
        Lang::En => ("Blackbox".to_owned(), "Notifications work. This is what an alert looks like.".to_owned()),
        Lang::Fr => ("Blackbox".to_owned(), "Les notifications fonctionnent. Voilà à quoi ressemble une alerte.".to_owned()),
    }
}

pub fn tray_tooltip(recording: bool, unread: u32, lang: Lang) -> String {
    let state = match (recording, lang) {
        (true, Lang::En) => "recording",
        (true, Lang::Fr) => "enregistrement",
        (false, Lang::En) => "paused",
        (false, Lang::Fr) => "en pause",
    };
    match (unread, lang) {
        (0, _) => format!("Blackbox · {state}"),
        (n, Lang::En) => format!("Blackbox · {state} · {n} new incident{}", if n == 1 { "" } else { "s" }),
        (1, Lang::Fr) => format!("Blackbox · {state} · 1 nouvel incident"),
        (n, Lang::Fr) => format!("Blackbox · {state} · {n} nouveaux incidents"),
    }
}

pub fn tray_menu(lang: Lang) -> TrayLabels {
    match lang {
        Lang::En => TrayLabels { open: "Open dashboard", pause: "Pause recording", resume: "Resume recording", quit: "Quit Blackbox" },
        Lang::Fr => TrayLabels {
            open: "Ouvrir le tableau de bord",
            pause: "Mettre en pause",
            resume: "Reprendre l'enregistrement",
            quit: "Quitter Blackbox",
        },
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TrayLabels {
    pub open: &'static str,
    pub pause: &'static str,
    pub resume: &'static str,
    pub quit: &'static str,
}
