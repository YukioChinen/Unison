// Teste do protocolo de bateria do mouse (VID 3554). So envia comandos de LEITURA:
//   0x03 = status, 0x04 = energia. Pacote de 17 bytes, ID 0x08, checksum = 0x55 - soma.
// Uso: cargo run --example mouse_probe
use hidapi::HidApi;
use std::time::{Duration, Instant};

const LEN: usize = 17;

fn packet(cmd: u8) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = 0x08;
    p[1] = cmd;
    let sum = p[..LEN - 1].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    p[LEN - 1] = 0x55u8.wrapping_sub(sum);
    p
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn main() {
    let api = HidApi::new().expect("hidapi");
    // PID opcional: mouse_probe.exe F54D (cabo) | F54F (dongle, padrao)
    let pid = std::env::args()
        .nth(1)
        .and_then(|s| u16::from_str_radix(&s, 16).ok())
        .unwrap_or(0xF54F);
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == 0x3554 && d.product_id() == pid && d.interface_number() == 1 && d.usage_page() == 0xFF02)
        .expect("interface vendor 0xFF02 do mouse nao encontrada");
    println!("abrindo {:04X}:{:04X} if1 page FF02", info.vendor_id(), info.product_id());
    let dev = info.open_device(&api).expect("abrir");

    for (name, cmd) in [("STATUS", 0x03u8), ("ENERGIA", 0x04u8)] {
        let p = packet(cmd);
        println!("\n> {name}: {}", hex(&p));
        match dev.write(&p) {
            Ok(n) => println!("  enviado ({n} bytes)"),
            Err(e) => {
                println!("  write falhou: {e}");
                continue;
            }
        }
        let t = Instant::now();
        let mut buf = [0u8; 64];
        let mut got = false;
        while t.elapsed() < Duration::from_millis(1500) {
            match dev.read_timeout(&mut buf, 200) {
                Ok(0) => {}
                Ok(n) => {
                    got = true;
                    println!("  < [{n}] {}", hex(&buf[..n]));
                    if n == LEN && buf[0] == 0x08 && buf[1] == cmd {
                        if cmd == 0x04 {
                            let mv = ((buf[8] as u16) << 8) | buf[9] as u16;
                            println!("    bateria={}%  flag[7]={}  tensao={} mV", buf[6], buf[7], mv);
                        } else {
                            println!("    disponivel={}", buf[6]);
                        }
                    }
                }
                Err(e) => {
                    println!("  read erro: {e}");
                    break;
                }
            }
        }
        if !got {
            println!("  (sem resposta)");
        }
    }
}
