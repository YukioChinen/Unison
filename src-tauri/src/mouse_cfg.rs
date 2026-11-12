//! Configuracao do mouse (polling, estagios de DPI, cores, iluminacao) pela memoria
//! de configuracao do mouse. Mesmo protocolo da bateria: pacote de 17 bytes, ID 0x08,
//! checksum = 0x55 - soma. Comandos: 0x08 = ler memoria, 0x07 = gravar memoria.
//!
//! Layout do pacote: [0]=0x08  [1]=cmd  [2]=status  [3..5]=endereco (big-endian)
//!                   [5]=tamanho (max 10)  [6..16]=dados  [16]=checksum
//!
//! Mapa da memoria (cada valor e seguido de um byte de verificacao = 85 - valor):
//!   0..10  : polling, qtd de estagios, estagio atual, bhop, modo dos botoes
//!   12..44 : 8 estagios de DPI, 4 bytes cada [x, y, extensao, 85-(x+y+ext)]
//!   44..76 : 8 cores dos estagios, 4 bytes cada [r, g, b, 85-(r+g+b)]
//!   76..84 : indicador de DPI: efeito, brilho, velocidade, ligado
//! (A memoria tambem tem campos de "tira de luz" em 84..96, mas este mouse nao tem tira: ignorados.)
//! Gravacoes sempre releem o bloco, alteram so o campo pedido e conferem por leitura.

use crate::mouse::{open_vendor_interface, preferred_pid};
use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LEN: usize = 17;
static LOCK: Mutex<()> = Mutex::new(());

const RATES: [(u8, u16); 7] = [(8, 125), (4, 250), (2, 500), (1, 1000), (16, 2000), (32, 4000), (64, 8000)];
const PID_WIRED: u16 = 0xF54D;
pub const MAX_STAGES: usize = 8;
/// Sensor PAW3395: faixa suportada de DPI, em passos de 50.
pub const DPI_MIN: u32 = 50;
pub const DPI_MAX: u32 = 26000;

#[derive(Clone, Serialize, Debug)]
pub struct Stage {
    pub dpi: u32,
    pub color: [u8; 3],
}

/// Indicador de DPI. effect: 0 = desligado, 1 = fixo, 2 = respirando.
/// brightness: 16 baixo / 128 medio / 255 alto. speed: 1 lenta / 3 media / 5 rapida.
#[derive(Clone, Serialize, Debug)]
pub struct DpiLight {
    pub effect: u8,
    pub brightness: u8,
    pub speed: u8,
}

#[derive(Clone, Serialize, Debug)]
pub struct MouseConfig {
    pub via: String,
    pub report_rate: u16,
    pub stage_count: u8,
    pub current_stage: u8,
    pub stages: Vec<Stage>,
    pub dpi_light: DpiLight,
}

// ---------- transporte ----------

fn checksum(p: &[u8]) -> u8 {
    0x55u8.wrapping_sub(p[..LEN - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b)))
}

fn open() -> Result<(HidDevice, String), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let pid = preferred_pid(&api).ok_or("Mouse não encontrado (dongle ou cabo).")?;
    let dev = open_vendor_interface(&api, pid).ok_or("Não foi possível abrir o mouse.")?;
    Ok((dev, if pid == PID_WIRED { "cabo" } else { "dongle" }.to_string()))
}

fn transact(dev: &HidDevice, cmd: u8, addr: u16, len: u8, data: &[u8]) -> Result<[u8; LEN], String> {
    // descarta respostas/eventos antigos (bateria etc.)
    let mut junk = [0u8; 64];
    while matches!(dev.read_timeout(&mut junk, 5), Ok(n) if n > 0) {}

    let mut req = [0u8; LEN];
    req[0] = 0x08;
    req[1] = cmd;
    req[3] = (addr >> 8) as u8;
    req[4] = addr as u8;
    req[5] = len;
    req[6..6 + data.len()].copy_from_slice(data);
    req[LEN - 1] = checksum(&req);
    dev.write(&req).map_err(|e| format!("falha ao enviar: {e}"))?;

    let t = Instant::now();
    let mut buf = [0u8; 64];
    while t.elapsed() < Duration::from_millis(1500) {
        if let Ok(n) = dev.read_timeout(&mut buf, 100) {
            if n == LEN && buf[0] == 0x08 && buf[1] == cmd && buf[LEN - 1] == checksum(&buf[..LEN]) {
                let mut out = [0u8; LEN];
                out.copy_from_slice(&buf[..LEN]);
                return Ok(out);
            }
        }
    }
    Err("O mouse não respondeu (ele pode estar dormindo; mexa nele e tente de novo).".into())
}

fn read_mem(dev: &HidDevice, addr: u16, len: u8) -> Result<Vec<u8>, String> {
    let r = transact(dev, 0x08, addr, len, &[])?;
    Ok(r[6..6 + len as usize].to_vec())
}

/// Grava e confere por leitura. `data` tem no maximo 10 bytes.
fn write_mem(dev: &HidDevice, addr: u16, data: &[u8]) -> Result<(), String> {
    transact(dev, 0x07, addr, data.len() as u8, data)?;
    let back = read_mem(dev, addr, data.len() as u8)?;
    if back != data {
        return Err(format!("Conferência falhou em @{addr}: gravado {data:02X?}, lido {back:02X?}"));
    }
    Ok(())
}

// ---------- codificacao ----------

fn pair_ok(v: u8, c: u8) -> bool {
    v.wrapping_add(c) == 85
}

fn crc1(v: u8) -> u8 {
    85u8.wrapping_sub(v)
}

/// DPI (PAW3395): 50 * (1 + x + 256*bits_altos). Extensao: bits 2-3 = altos de X, 6-7 = altos de Y.
fn decode_dpi(x: u8, y: u8, ext: u8) -> u32 {
    let xv = x as u32 | (((ext & 0x0C) >> 2) as u32) << 8;
    let _ = y; // o app mantem X e Y iguais; mostramos o valor de X
    let mut dx = 50 * (1 + xv);
    if ext & 1 != 0 {
        dx *= 2;
    }
    dx
}

fn encode_dpi(dpi: u32) -> Result<[u8; 4], String> {
    if !(DPI_MIN..=DPI_MAX).contains(&dpi) || dpi % 50 != 0 {
        return Err(format!("DPI inválido: use múltiplos de 50 entre {DPI_MIN} e {DPI_MAX}."));
    }
    let a = dpi / 50 - 1;
    let low = (a & 0xFF) as u8;
    let high = ((a >> 8) & 3) as u8;
    let ext = (high << 6) | (high << 2);
    Ok([low, low, ext, crc1(low.wrapping_add(low).wrapping_add(ext))])
}

fn rate_from_code(code: u8) -> Option<u16> {
    RATES.iter().find(|(c, _)| *c == code).map(|(_, hz)| *hz)
}

// ---------- operacoes ----------

pub fn read_config() -> Result<MouseConfig, String> {
    let _g = LOCK.lock().unwrap();
    let (dev, via) = open()?;

    let b0 = read_mem(&dev, 0, 10)?;
    if !(pair_ok(b0[0], b0[1]) && pair_ok(b0[2], b0[3]) && pair_ok(b0[4], b0[5])) {
        return Err("Configuração do mouse com verificação inválida (protocolo diferente?).".into());
    }
    let report_rate = rate_from_code(b0[0]).ok_or(format!("Código de polling desconhecido: {}", b0[0]))?;

    let mut stages = Vec::new();
    for i in 0..4u16 {
        let d = read_mem(&dev, 12 + 8 * i, 8)?;
        let c = read_mem(&dev, 44 + 8 * i, 8)?;
        for s in 0..2usize {
            let (p, q) = (&d[s * 4..s * 4 + 4], &c[s * 4..s * 4 + 4]);
            stages.push(Stage {
                dpi: decode_dpi(p[0], p[1], p[2]),
                color: [q[0], q[1], q[2]],
            });
        }
    }

    let l = read_mem(&dev, 76, 8)?;
    if !(0..4).all(|i| pair_ok(l[i * 2], l[i * 2 + 1])) {
        return Err("Iluminação do DPI com verificação inválida.".into());
    }
    let dpi_light = DpiLight { effect: if l[6] == 1 { l[0] } else { 0 }, brightness: l[2], speed: l[4] };

    Ok(MouseConfig {
        via,
        report_rate,
        stage_count: b0[2].clamp(1, MAX_STAGES as u8),
        current_stage: b0[4],
        stages,
        dpi_light,
    })
}

fn with_block0(f: impl FnOnce(&mut Vec<u8>) -> Result<(), String>) -> Result<(), String> {
    let _g = LOCK.lock().unwrap();
    let (dev, _) = open()?;
    let mut b0 = read_mem(&dev, 0, 10)?;
    if !(pair_ok(b0[0], b0[1]) && pair_ok(b0[2], b0[3]) && pair_ok(b0[4], b0[5])) {
        return Err("Configuração atual inválida; nada foi gravado.".into());
    }
    f(&mut b0)?;
    write_mem(&dev, 0, &b0)
}

pub fn set_report_rate(hz: u16) -> Result<(), String> {
    let code = RATES.iter().find(|(_, h)| *h == hz).map(|(c, _)| *c).ok_or(format!("Polling não suportado: {hz} Hz"))?;
    with_block0(|b| {
        b[0] = code;
        b[1] = crc1(code);
        Ok(())
    })
}

pub fn set_current_stage(index: u8) -> Result<(), String> {
    with_block0(|b| {
        if index >= b[2] {
            return Err(format!("Estágio {} não existe (o mouse tem {}).", index + 1, b[2]));
        }
        b[4] = index;
        b[5] = crc1(index);
        Ok(())
    })
}

pub fn set_stage_count(count: u8) -> Result<(), String> {
    if !(1..=MAX_STAGES as u8).contains(&count) {
        return Err("A quantidade de estágios deve ficar entre 1 e 8.".into());
    }
    with_block0(|b| {
        b[2] = count;
        b[3] = crc1(count);
        if b[4] >= count {
            b[4] = count - 1;
            b[5] = crc1(b[4]);
        }
        Ok(())
    })
}

/// Indicador de DPI: effect 0 = desligar (preserva o efeito salvo), 1 = fixo, 2 = respirando.
pub fn set_dpi_light(effect: u8, brightness: u8, speed: u8) -> Result<(), String> {
    if effect > 2 {
        return Err("Efeito do indicador de DPI inválido.".into());
    }
    if ![16, 128, 255].contains(&brightness) {
        return Err("Brilho do indicador deve ser baixo (16), médio (128) ou alto (255).".into());
    }
    if ![1, 3, 5].contains(&speed) {
        return Err("Velocidade do indicador deve ser 1, 3 ou 5.".into());
    }
    let _g = LOCK.lock().unwrap();
    let (dev, _) = open()?;
    let mut l = read_mem(&dev, 76, 8)?;
    if !(0..4).all(|i| pair_ok(l[i * 2], l[i * 2 + 1])) {
        return Err("Iluminação atual inválida; nada foi gravado.".into());
    }
    if effect == 0 {
        l[6] = 0;
        l[7] = crc1(0);
    } else {
        l[0] = effect;
        l[1] = crc1(effect);
        l[6] = 1;
        l[7] = crc1(1);
    }
    l[2] = brightness;
    l[3] = crc1(brightness);
    l[4] = speed;
    l[5] = crc1(speed);
    write_mem(&dev, 76, &l)
}

/// Altera DPI e cor de um estagio (0..7), regravando o par de estagios vizinhos.
pub fn set_stage(index: u8, dpi: u32, color: [u8; 3]) -> Result<(), String> {
    if index as usize >= MAX_STAGES {
        return Err("Estágio inválido.".into());
    }
    let enc = encode_dpi(dpi)?;
    let _g = LOCK.lock().unwrap();
    let (dev, _) = open()?;
    let pair = (index / 2) as u16;
    let slot = (index % 2) as usize * 4;

    let mut d = read_mem(&dev, 12 + 8 * pair, 8)?;
    d[slot..slot + 4].copy_from_slice(&enc);
    write_mem(&dev, 12 + 8 * pair, &d)?;

    let mut c = read_mem(&dev, 44 + 8 * pair, 8)?;
    let sum = color[0].wrapping_add(color[1]).wrapping_add(color[2]);
    c[slot..slot + 4].copy_from_slice(&[color[0], color[1], color[2], crc1(sum)]);
    write_mem(&dev, 44 + 8 * pair, &c)
}
