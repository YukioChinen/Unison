pub mod headset;
pub mod mouse;
pub mod mouse_cfg;
pub mod tray;

use headset::HeadsetBattery;
use hidapi::HidApi;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

struct HeadsetShared(Arc<Mutex<HeadsetBattery>>);
struct MouseShared(Arc<Mutex<mouse::MouseBattery>>);

// ---- iniciar com o Windows ----
#[tauri::command]
fn autostart_get(app: tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn autostart_set(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    tray::set_autostart(&app, enabled)
}

// ---- configuracao do mouse: operacoes bloqueantes fora da thread da interface ----
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn mouse_get_config() -> Result<mouse_cfg::MouseConfig, String> {
    blocking(mouse_cfg::read_config).await
}

#[tauri::command]
async fn mouse_set_report_rate(hz: u16) -> Result<(), String> {
    blocking(move || mouse_cfg::set_report_rate(hz)).await
}

#[tauri::command]
async fn mouse_set_stage(index: u8, dpi: u32, color: [u8; 3]) -> Result<(), String> {
    blocking(move || mouse_cfg::set_stage(index, dpi, color)).await
}

#[tauri::command]
async fn mouse_set_dpi_light(effect: u8, brightness: u8, speed: u8) -> Result<(), String> {
    blocking(move || mouse_cfg::set_dpi_light(effect, brightness, speed)).await
}

#[tauri::command]
async fn mouse_set_current_stage(index: u8) -> Result<(), String> {
    blocking(move || mouse_cfg::set_current_stage(index)).await
}

#[tauri::command]
async fn mouse_set_stage_count(count: u8) -> Result<(), String> {
    blocking(move || mouse_cfg::set_stage_count(count)).await
}

#[tauri::command]
fn get_mouse_battery(state: tauri::State<MouseShared>) -> mouse::MouseBattery {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
fn get_headset_battery(state: tauri::State<HeadsetShared>) -> HeadsetBattery {
    state.0.lock().unwrap().clone()
}

/// Fabricantes dos dispositivos suportados.
const KNOWN_VIDS: [u16; 2] = [
    0x3554, // Redragon M916 PRO 4K (dongle 2.4G)
    0x373B, // ATK N9 Ultra (dongle)
];

#[derive(Serialize)]
struct HidInfo {
    vendor_id: u16,
    product_id: u16,
    product: String,
    manufacturer: String,
    interface: i32,
    usage_page: u16,
    usage: u16,
    path: String,
}

/// Lista as interfaces HID. Com `only_known = true`, mostra so mouse e headset.
#[tauri::command]
fn list_hid_devices(only_known: bool) -> Result<Vec<HidInfo>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let mut out: Vec<HidInfo> = api
        .device_list()
        .filter(|d| !only_known || KNOWN_VIDS.contains(&d.vendor_id()))
        .map(|d| HidInfo {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            product: d.product_string().unwrap_or("").to_string(),
            manufacturer: d.manufacturer_string().unwrap_or("").to_string(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            path: d.path().to_string_lossy().into_owned(),
        })
        .collect();
    out.sort_by_key(|d| (d.vendor_id, d.product_id, d.interface, d.usage_page, d.usage));
    Ok(out)
}

/// Escuta relatorios de entrada de uma interface por alguns segundos.
/// Serve para descobrir qual relatorio carrega a bateria.
#[tauri::command]
fn sniff_reports(path: String, millis: u64) -> Result<Vec<String>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let cpath = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
    let dev = api.open_path(&cpath).map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    let mut reports = Vec::new();
    let mut buf = [0u8; 128];
    while start.elapsed().as_millis() < millis as u128 && reports.len() < 500 {
        match dev.read_timeout(&mut buf, 100) {
            Ok(0) => {}
            Ok(n) => reports.push(
                buf[..n]
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(reports)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Instancia unica: uma segunda abertura so traz a janela existente para frente
        // (duas instancias brigariam pela porta serial do headset).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_window(app)))
        // Iniciar com o Windows: o sistema abre o app com --minimized (direto na bandeja).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        // Fechar a janela a DESTROI (libera o WebView2, que e o que mais gasta memoria);
        // o app segue vivo na bandeja e recria a janela quando for aberto de novo.
        .setup(|app| {
            tray::init(app.handle())?;
            // Nao ha janela criada na partida; so abre se nao veio da inicializacao do Windows.
            if !std::env::args().any(|a| a == "--minimized") {
                tray::show_window(app.handle());
            }
            let shared = Arc::new(Mutex::new(HeadsetBattery::default()));
            let mouse_shared = Arc::new(Mutex::new(mouse::MouseBattery::default()));
            app.manage(HeadsetShared(shared.clone()));
            let handle = app.handle().clone();
            let (hs_for_hs, ms_for_hs) = (shared.clone(), mouse_shared.clone());
            let (hs_for_ms, ms_for_ms) = (shared.clone(), mouse_shared.clone());
            let rate_path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("headset_charge_rate.txt"));
            std::thread::spawn(move || {
                headset::run(rate_path, move |s| {
                    *hs_for_hs.lock().unwrap() = s.clone();
                    let _ = handle.emit("headset-battery", s);
                    tray::update(&handle, &hs_for_hs, &ms_for_hs);
                });
            });

            app.manage(MouseShared(mouse_shared.clone()));
            let mouse_handle = app.handle().clone();
            std::thread::spawn(move || {
                mouse::run(move |s| {
                    *ms_for_ms.lock().unwrap() = s.clone();
                    let _ = mouse_handle.emit("mouse-battery", s);
                    tray::update(&mouse_handle, &hs_for_ms, &ms_for_ms);
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hid_devices,
            sniff_reports,
            get_headset_battery,
            get_mouse_battery,
            autostart_get,
            autostart_set,
            mouse_get_config,
            mouse_set_report_rate,
            mouse_set_stage,
            mouse_set_dpi_light,
            mouse_set_current_stage,
            mouse_set_stage_count
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Unison")
        .run(|_app, event| {
            // Sem janelas o Tauri quereria encerrar; so "Sair" (codigo explicito) encerra de verdade.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
