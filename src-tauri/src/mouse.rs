//! Redragon M916 PRO 4K (dongle 2.4G, VID 0x3554): bateria por HID.
//!
//! Pacote de 17 bytes, report ID 0x08, checksum no ultimo byte = 0x55 - soma dos 16 primeiros.
//! Somente comandos de LEITURA sao usados:
//! - 0x03 STATUS : resposta[6] = 1 se o mouse esta disponivel no dongle
//! - 0x04 ENERGIA: resposta[6] = bateria %, [7] = flag de energia, [8..10] = tensao (mV, big-endian)
//! O mouse tambem envia sozinho um evento (cmd 0x0A, [6] = 0x40) quando a energia muda.
//! Protocolo conferido no mouse real; a semantica do byte [7] durante a carga ainda e validada.

use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::time::{Duration, Instant};

pub const VID: u16 = 0x3554;
/// Com o cabo plugado o mouse sai do dongle e aparece com outro PID; preferimos o cabo.
const PID_WIRED: u16 = 0xF54D;
const PID_DONGLE: u16 = 0xF54F;
const LEN: usize = 17;
const POLL_EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Serialize, Debug)]
pub struct MouseBattery {
    /// "ok" | "not_found" | "open_failed" | "no_reply"
    pub status: String,
    /// Mouse ligado e pareado com o dongle.
    pub available: bool,
    pub percent: Option<u8>,
    pub voltage_mv: Option<u16>,
    /// Byte [7] da resposta de energia (1 = ligado ao cabo / carregando).
    pub power_flag: Option<u8>,
    pub charging: bool,
    /// Lido pelo cabo (true) ou pelo dongle 2.4G (false).
    pub wired: bool,
}

impl Default for MouseBattery {
    fn default() -> Self {
        Self { status: "not_found".into(), available: false, percent: None, voltage_mv: None, power_flag: None, charging: false, wired: false }
    }
}

fn packet(cmd: u8) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = 0x08;
    p[1] = cmd;
    p[LEN - 1] = checksum(&p);
    p
}

fn checksum(p: &[u8]) -> u8 {
    let sum = p[..LEN - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    0x55u8.wrapping_sub(sum)
}

/// PID preferido presente agora: cabo primeiro, depois dongle.
pub(crate) fn preferred_pid(api: &HidApi) -> Option<u16> {
    let has = |pid: u16| {
        api.device_list()
            .any(|d| d.vendor_id() == VID && d.product_id() == pid && d.interface_number() == 1 && d.usage_page() == 0xFF02)
    };
    [PID_WIRED, PID_DONGLE].into_iter().find(|&p| has(p))
}

pub(crate) fn open_vendor_interface(api: &HidApi, pid: u16) -> Option<HidDevice> {
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == pid && d.interface_number() == 1 && d.usage_page() == 0xFF02)?;
    info.open_device(api).ok()
}

fn publish(s: &MouseBattery, last: &mut MouseBattery, f: &mut dyn FnMut(&MouseBattery)) {
    if serde_json::to_string(s).ok() != serde_json::to_string(&*last).ok() {
        *last = s.clone();
        f(s);
    }
}

/// Laco principal: nunca retorna. Chama `on_update` a cada mudanca de estado.
pub fn run(mut on_update: impl FnMut(&MouseBattery)) {
    let mut state = MouseBattery::default();
    let mut last = state.clone();

    loop {
        let mut api = match HidApi::new() {
            Ok(a) => a,
            Err(_) => {
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        let Some(pid) = preferred_pid(&api) else {
            state = MouseBattery { status: "not_found".into(), ..Default::default() };
            publish(&state, &mut last, &mut on_update);
            std::thread::sleep(Duration::from_secs(3));
            continue;
        };
        let Some(dev) = open_vendor_interface(&api, pid) else {
            state.status = "open_failed".into();
            publish(&state, &mut last, &mut on_update);
            std::thread::sleep(Duration::from_secs(3));
            continue;
        };
        // Trocou de dongle para cabo (ou vice-versa): descarta leituras antigas.
        state.wired = pid == PID_WIRED;
        state.percent = None;
        state.voltage_mv = None;
        state.power_flag = None;

        let mut switched = false;
        let mut next_poll = Instant::now();
        let mut last_rx = Instant::now();
        'session: loop {
            if Instant::now() >= next_poll {
                // reavalia qual conexao esta ativa; se mudou, reabre
                let _ = api.refresh_devices();
                if preferred_pid(&api) != Some(pid) {
                    switched = true;
                    break 'session;
                }
                for cmd in [0x03u8, 0x04u8] {
                    if dev.write(&packet(cmd)).is_err() {
                        break 'session;
                    }
                }
                next_poll = Instant::now() + POLL_EVERY;
            }

            let mut buf = [0u8; 64];
            match dev.read_timeout(&mut buf, 300) {
                Ok(0) => {}
                Ok(n) if n == LEN && buf[0] == 0x08 && buf[LEN - 1] == checksum(&buf[..LEN]) => {
                    last_rx = Instant::now();
                    state.status = "ok".into();
                    match buf[1] {
                        0x03 => state.available = buf[6] == 1,
                        0x04 => {
                            if buf[6] <= 100 {
                                state.percent = Some(buf[6]);
                            }
                            state.power_flag = Some(buf[7]);
                            state.charging = buf[7] == 1;
                            state.voltage_mv = Some(((buf[8] as u16) << 8) | buf[9] as u16);
                        }
                        // evento espontaneo de mudanca de energia: consulta agora
                        0x0A if buf[6] == 0x40 => next_poll = Instant::now(),
                        _ => {}
                    }
                    publish(&state, &mut last, &mut on_update);
                }
                Ok(_) => {}
                Err(_) => break 'session,
            }

            if last_rx.elapsed() > Duration::from_secs(20) {
                state.status = "no_reply".into();
                publish(&state, &mut last, &mut on_update);
                last_rx = Instant::now();
            }
        }
        // Dispositivo sumiu (cabo plugado/desplugado): troca de conexao, nao e erro.
        let _ = api.refresh_devices();
        if preferred_pid(&api) != Some(pid) {
            switched = true;
        }
        if !switched {
            state.status = "open_failed".into();
            publish(&state, &mut last, &mut on_update);
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}
