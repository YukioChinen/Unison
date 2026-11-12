//! ATK N9 Ultra (SoundMai): protocolo pela porta serial do dongle, 9600 8N1.
//!
//! Pacotes de 64 bytes: [len, cmd, sub, dados...]
//! - Pedido de bateria: 02 00 03 (resto zerado). Resposta (sub 0x03) ou aviso
//!   espontaneo (sub 0x83): byte 4 = bit 7 carregando | bits 0-6 porcentagem.
//! - Pedido de status: 02 00 04. Resposta: byte 4 = 1 se o headset esta conectado.
//!
//! O headset informa 100% enquanto carrega, entao a porcentagem durante a carga
//! e ESTIMADA a partir do ultimo nivel real e de uma taxa de carga que se ajusta sozinha.

use serde::Serialize;
use serialport::SerialPortType;
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const VID: u16 = 0x373B;
pub const PID: u16 = 0x1204;

const PACKET: usize = 64;
const POLL_EVERY: Duration = Duration::from_secs(5);
const DEFAULT_RATE: f32 = 0.8; // % por minuto (~2 h para carga completa)

#[derive(Clone, Serialize, Debug)]
pub struct HeadsetBattery {
    /// "ok" | "not_found" | "port_busy" | "no_reply"
    pub status: String,
    pub headset_on: bool,
    pub charging: bool,
    /// Valor cru informado pelo headset (100 enquanto carrega).
    pub raw_percent: Option<u8>,
    /// Valor para exibir: real descarregando, estimado carregando.
    pub percent: Option<f32>,
    pub estimated: bool,
    pub charge_rate: f32,
}

impl Default for HeadsetBattery {
    fn default() -> Self {
        Self {
            status: "not_found".into(),
            headset_on: false,
            charging: false,
            raw_percent: None,
            percent: None,
            estimated: false,
            charge_rate: DEFAULT_RATE,
        }
    }
}

// ---------- estimativa ----------

fn median(v: &[u8]) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    Some(s[s.len() / 2] as f32)
}

struct Estimator {
    rate: f32,
    rate_path: Option<PathBuf>,
    recent: VecDeque<u8>,
    charging_since: Option<Instant>,
    start_level: Option<f32>,
    /// (nivel ao plugar, minutos carregados) aguardando leituras reais ao desplugar.
    pending_cal: Option<(f32, f32)>,
    post: Vec<u8>,
}

impl Estimator {
    fn new(rate_path: Option<PathBuf>) -> Self {
        let rate = rate_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|r| (0.2..=3.0).contains(r))
            .unwrap_or(DEFAULT_RATE);
        Self { rate, rate_path, recent: VecDeque::new(), charging_since: None, start_level: None, pending_cal: None, post: vec![] }
    }

    fn save_rate(&self) {
        if let Some(p) = &self.rate_path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, format!("{:.4}", self.rate));
        }
    }

    /// Devolve (porcentagem para exibir, e estimada?)
    fn update(&mut self, raw: u8, charging: bool, now: Instant) -> (Option<f32>, bool) {
        if !charging {
            if let Some(since) = self.charging_since.take() {
                // Acabou de desplugar: prepara calibracao com as proximas leituras reais.
                if let Some(start) = self.start_level {
                    self.pending_cal = Some((start, now.duration_since(since).as_secs_f32() / 60.0));
                    self.post.clear();
                }
                self.recent.clear();
                self.start_level = None;
            }
            if let Some((start, minutes)) = self.pending_cal {
                self.post.push(raw);
                if self.post.len() >= 3 {
                    if let Some(actual) = median(&self.post) {
                        if minutes >= 10.0 && actual > start + 3.0 {
                            let measured = ((actual - start) / minutes).clamp(0.2, 3.0);
                            self.rate = 0.5 * self.rate + 0.5 * measured;
                            self.save_rate();
                        }
                    }
                    self.pending_cal = None;
                    self.post.clear();
                }
            }
            self.recent.push_back(raw);
            if self.recent.len() > 5 {
                self.recent.pop_front();
            }
            let v: Vec<u8> = self.recent.iter().copied().collect();
            return (median(&v), false);
        }

        // Carregando
        if self.charging_since.is_none() {
            self.charging_since = Some(now);
            let v: Vec<u8> = self.recent.iter().copied().collect();
            self.start_level = median(&v);
            self.pending_cal = None;
        }
        let minutes = now.duration_since(self.charging_since.unwrap()).as_secs_f32() / 60.0;
        (self.start_level.map(|s| (s + self.rate * minutes).min(99.0)), true)
    }
}

// ---------- serial ----------

fn find_port() -> Option<String> {
    serialport::available_ports().ok()?.into_iter().find_map(|p| match p.port_type {
        SerialPortType::UsbPort(u) if u.vid == VID && u.pid == PID => Some(p.port_name),
        _ => None,
    })
}

fn request(sub: u8) -> [u8; PACKET] {
    let mut r = [0u8; PACKET];
    r[0] = 2;
    r[2] = sub;
    r
}

/// Laco principal: nunca retorna. Chama `on_update` a cada mudanca de estado.
pub fn run(rate_path: Option<PathBuf>, mut on_update: impl FnMut(&HeadsetBattery)) {
    let mut est = Estimator::new(rate_path);
    let mut state = HeadsetBattery { charge_rate: est.rate, ..Default::default() };
    let mut last_sent = state.clone();
    let publish = |s: &HeadsetBattery, last: &mut HeadsetBattery, f: &mut dyn FnMut(&HeadsetBattery)| {
        let a = serde_json::to_string(s).unwrap_or_default();
        let b = serde_json::to_string(&*last).unwrap_or_default();
        if a != b {
            *last = s.clone();
            f(s);
        }
    };

    loop {
        let name = match find_port() {
            Some(n) => n,
            None => {
                state.status = "not_found".into();
                publish(&state, &mut last_sent, &mut on_update);
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        let mut port = match serialport::new(&name, 9600).timeout(Duration::from_millis(300)).open() {
            Ok(p) => p,
            Err(_) => {
                state.status = "port_busy".into();
                publish(&state, &mut last_sent, &mut on_update);
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };

        let mut buf: Vec<u8> = Vec::new();
        let mut next_poll = Instant::now();
        let mut last_rx = Instant::now();
        'session: loop {
            if Instant::now() >= next_poll {
                for sub in [3u8, 4u8] {
                    if port.write_all(&request(sub)).is_err() {
                        break 'session;
                    }
                }
                next_poll = Instant::now() + POLL_EVERY;
            }

            let mut tmp = [0u8; 256];
            match port.read(&mut tmp) {
                Ok(n) if n > 0 => buf.extend_from_slice(&tmp[..n]),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::TimedOut => {}
                Err(_) => break 'session,
            }

            while buf.len() >= PACKET {
                let pkt: Vec<u8> = buf.drain(..PACKET).collect();
                last_rx = Instant::now();
                state.status = "ok".into();
                match pkt[2] {
                    0x03 | 0x83 => {
                        let r = pkt[4];
                        let charging = r >> 7 == 1;
                        let raw = r & 0x7F;
                        let (pct, estimated) = est.update(raw, charging, Instant::now());
                        state.charging = charging;
                        state.raw_percent = Some(raw);
                        state.percent = pct;
                        state.estimated = estimated;
                        state.charge_rate = est.rate;
                    }
                    0x04 => state.headset_on = pkt[4] == 1,
                    _ => {}
                }
                publish(&state, &mut last_sent, &mut on_update);
            }

            if last_rx.elapsed() > Duration::from_secs(20) {
                state.status = "no_reply".into();
                publish(&state, &mut last_sent, &mut on_update);
                last_rx = Instant::now();
            }
        }
        // sessao caiu: tenta reabrir
        state.status = "port_busy".into();
        publish(&state, &mut last_sent, &mut on_update);
        std::thread::sleep(Duration::from_secs(2));
    }
}
