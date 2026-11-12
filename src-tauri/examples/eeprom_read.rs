// Leitura (SOMENTE LEITURA) da memoria de configuracao do mouse via GetEEPROM (cmd 0x08).
// Uso: cargo run --example eeprom_read -- [pid_hex]   (padrao F54F = dongle; F54D = cabo)
use hidapi::HidApi;
use std::time::{Duration, Instant};

const LEN: usize = 17;

fn packet(cmd: u8, addr: u16, len: u8) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = 0x08;
    p[1] = cmd;
    p[3] = (addr >> 8) as u8;
    p[4] = addr as u8;
    p[5] = len;
    let sum = p[..LEN - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    p[LEN - 1] = 0x55u8.wrapping_sub(sum);
    p
}

fn checksum_ok(p: &[u8]) -> bool {
    let sum = p[..LEN - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    p[LEN - 1] == 0x55u8.wrapping_sub(sum)
}

fn main() {
    let pid = std::env::args().nth(1).and_then(|s| u16::from_str_radix(&s, 16).ok()).unwrap_or(0xF54F);
    let api = HidApi::new().expect("hidapi");
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == 0x3554 && d.product_id() == pid && d.interface_number() == 1 && d.usage_page() == 0xFF02)
        .expect("interface 0xFF02 nao encontrada");
    let dev = info.open_device(&api).expect("abrir");
    println!("lendo via {:04X}:{:04X}\n", info.vendor_id(), info.product_id());

    // (rotulo, endereco, tamanho)
    let blocks: [(&str, u16, u8); 12] = [
        ("polling/DPI/bhop/botoes", 0, 10),
        ("dpi 1-2", 12, 8),
        ("dpi 3-4", 20, 8),
        ("dpi 5-6", 28, 8),
        ("dpi 7-8", 36, 8),
        ("cor dpi 1-2", 44, 8),
        ("cor dpi 3-4", 52, 8),
        ("cor dpi 5-6", 60, 8),
        ("cor dpi 7-8", 68, 8),
        ("luz DPI (efeito/brilho/vel/on)", 76, 8),
        ("luz lateral R,G,B,crc,efeito", 84, 10),
        ("luz lateral brilho/vel/economia", 90, 6),
    ];

    for (name, addr, len) in blocks {
        let req = packet(0x08, addr, len);
        dev.write(&req).expect("write");
        let t = Instant::now();
        let mut buf = [0u8; 64];
        let mut shown = false;
        while t.elapsed() < Duration::from_millis(1200) {
            match dev.read_timeout(&mut buf, 150) {
                Ok(n) if n == LEN && buf[1] == 0x08 => {
                    let data = &buf[6..6 + len as usize];
                    let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
                    println!(
                        "{:<34} @{:<3} len={:<2} {}  (checksum pacote: {})",
                        name,
                        addr,
                        len,
                        hex.join(" "),
                        if checksum_ok(&buf[..LEN]) { "ok" } else { "ERRO" }
                    );
                    shown = true;
                    break;
                }
                _ => {}
            }
        }
        if !shown {
            println!("{name:<34} @{addr:<3} (sem resposta)");
        }
    }
}
