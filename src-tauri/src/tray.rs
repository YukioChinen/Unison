//! Icone na bandeja do sistema: mostra o menor nivel entre mouse e headset,
//! com tooltip e menu detalhados. Clique esquerdo abre a janela.

use crate::headset::HeadsetBattery;
use crate::mouse::MouseBattery;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri_plugin_autostart::ManagerExt;
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

pub struct TrayItems {
    headset: MenuItem<Wry>,
    mouse: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

/// Liga/desliga o inicio com o Windows (entrada do usuario em HKCU\...\Run) e
/// sincroniza o item do menu. Devolve o estado real depois da mudanca.
pub fn set_autostart(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    let actual = manager.is_enabled().unwrap_or(false);
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.autostart.set_checked(actual);
    }
    Ok(actual)
}

pub const TRAY_ID: &str = "unison";

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let headset = MenuItem::with_id(app, "hs", "Headset: …", false, None::<&str>)?;
    let mouse = MenuItem::with_id(app, "ms", "Mouse: …", false, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "Abrir Unison", true, None::<&str>)?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Iniciar com o Windows", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&headset, &mouse, &sep, &show, &autostart, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(None, false))
        .tooltip("Unison")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "show" => show_window(app),
            "autostart" => {
                // o clique ja alternou a marca; aplica o inverso do estado real
                let now = app.autolaunch().is_enabled().unwrap_or(false);
                let _ = set_autostart(app, !now);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayItems { headset, mouse, autostart });
    Ok(())
}

/// Traz a janela para frente; se ela foi destruida (para economizar memoria), recria.
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    // Criar janela dentro de callbacks do laco de eventos pode travar no Windows:
    // faz em outra thread e evita criar duas vezes se chamado em sequencia.
    static CREATING: AtomicBool = AtomicBool::new(false);
    if CREATING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if app.get_webview_window("main").is_none() {
            if let Some(cfg) = app.config().app.windows.first().cloned() {
                if let Ok(builder) = tauri::WebviewWindowBuilder::from_config(&app, &cfg) {
                    if let Ok(w) = builder.visible(true).build() {
                        let _ = w.set_focus();
                    }
                }
            }
        }
        CREATING.store(false, Ordering::SeqCst);
    });
}

fn headset_text(h: &HeadsetBattery) -> String {
    match (h.status.as_str(), h.percent) {
        ("ok", Some(p)) => {
            let est = if h.estimated { "~" } else { "" };
            let chg = if h.charging { " ⚡" } else { "" };
            let off = if h.headset_on { "" } else { " (desligado)" };
            format!("Headset: {est}{}%{chg}{off}", p.round() as i32)
        }
        ("ok", None) if h.charging => "Headset: carregando ⚡".into(),
        ("port_busy", _) => "Headset: porta ocupada (feche o ATK HUB)".into(),
        _ => "Headset: indisponível".into(),
    }
}

fn mouse_text(m: &MouseBattery) -> String {
    match (m.status.as_str(), m.percent) {
        ("ok", Some(p)) => {
            let chg = if m.charging { " ⚡" } else { "" };
            let off = if m.available { "" } else { " (desligado)" };
            format!("Mouse: {p}%{chg}{off}")
        }
        _ => "Mouse: indisponível".into(),
    }
}

/// Valor mostrado no icone: o menor nivel entre os aparelhos disponiveis.
fn icon_value(h: &HeadsetBattery, m: &MouseBattery) -> (Option<u8>, bool) {
    let mut v: Vec<(u8, bool)> = vec![];
    if h.status == "ok" && h.headset_on {
        if let Some(p) = h.percent {
            v.push((p.round().clamp(0.0, 100.0) as u8, h.charging));
        }
    }
    if m.status == "ok" && m.available {
        if let Some(p) = m.percent {
            v.push((p, m.charging));
        }
    }
    match v.into_iter().min_by_key(|(p, _)| *p) {
        Some((p, c)) => (Some(p), c),
        None => (None, false),
    }
}

pub fn update(app: &AppHandle, hs: &Arc<Mutex<HeadsetBattery>>, ms: &Arc<Mutex<MouseBattery>>) {
    let h = hs.lock().unwrap().clone();
    let m = mouse_snapshot(ms);
    let (ht, mt) = (headset_text(&h), mouse_text(&m));

    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.headset.set_text(&ht);
        let _ = items.mouse.set_text(&mt);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("Unison\n{ht}\n{mt}")));
        let (val, chg) = icon_value(&h, &m);
        let _ = tray.set_icon(Some(icon(val, chg)));
    }
}

fn mouse_snapshot(ms: &Arc<Mutex<MouseBattery>>) -> MouseBattery {
    ms.lock().unwrap().clone()
}

// ---------- icone desenhado em pixels ----------

const FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

pub fn icon(value: Option<u8>, charging: bool) -> Image<'static> {
    const S: usize = 32;
    let bg: [u8; 3] = match (value, charging) {
        (None, _) => [90, 96, 110],
        (_, true) => [59, 130, 246],
        (Some(v), _) if v > 40 => [46, 160, 100],
        (Some(v), _) if v > 20 => [214, 150, 30],
        _ => [200, 60, 60],
    };
    let mut px = vec![0u8; S * S * 4];
    // fundo: quadrado de cantos arredondados
    for y in 0..S {
        for x in 0..S {
            let (cx, cy) = (x.min(S - 1 - x), y.min(S - 1 - y));
            let r = 6usize;
            let inside = !(cx < r && cy < r && (r - cx) * (r - cx) + (r - cy) * (r - cy) > r * r);
            if inside {
                let i = (y * S + x) * 4;
                px[i..i + 3].copy_from_slice(&bg);
                px[i + 3] = 255;
            }
        }
    }
    let mut put = |x: usize, y: usize| {
        if x < S && y < S {
            let i = (y * S + x) * 4;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    };
    match value {
        None => {
            for x in 9..23 {
                for y in 15..18 {
                    put(x, y);
                }
            }
        }
        Some(v) => {
            let s = v.to_string();
            let digits: Vec<usize> = s.bytes().map(|b| (b - b'0') as usize).collect();
            let scale = match digits.len() {
                1 => 5,
                2 => 4,
                _ => 2,
            };
            let w = digits.len() * 3 * scale + (digits.len() - 1) * scale;
            let h = 5 * scale;
            let (x0, y0) = ((S - w) / 2, (S - h) / 2);
            for (di, d) in digits.iter().enumerate() {
                for (row, bits) in FONT[*d].iter().enumerate() {
                    for col in 0..3 {
                        if bits & (0b100 >> col) != 0 {
                            for dy in 0..scale {
                                for dx in 0..scale {
                                    put(x0 + di * 4 * scale + col * scale + dx, y0 + row * scale + dy);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Image::new_owned(px, S as u32, S as u32)
}
