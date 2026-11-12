// Escuta todas as interfaces proprietarias (usage page >= 0xFF00) do mouse e do headset.
// Uso: cargo run --example sniff -- <segundos>
// Imprime so relatorios que mudaram, com o tempo desde o inicio.
use hidapi::HidApi;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let api = HidApi::new().expect("hidapi");
    let start = Instant::now();
    let out = Arc::new(Mutex::new(()));
    let mut handles = vec![];

    for d in api.device_list() {
        if !(d.vendor_id() == 0x3554 || d.vendor_id() == 0x373B) || d.usage_page() < 0xFF00 {
            continue;
        }
        let tag = format!(
            "{:04X} if{} p{:04X}/u{:02X}",
            d.vendor_id(),
            d.interface_number(),
            d.usage_page(),
            d.usage()
        );
        match d.open_device(&api) {
            Ok(dev) => {
                println!("[aberto] {tag}");
                let out = out.clone();
                handles.push(std::thread::spawn(move || {
                    let mut buf = [0u8; 256];
                    let mut last: Vec<u8> = vec![];
                    while start.elapsed() < Duration::from_secs(secs) {
                        match dev.read_timeout(&mut buf, 200) {
                            Ok(0) => {}
                            Ok(n) => {
                                if buf[..n] != last[..] {
                                    last = buf[..n].to_vec();
                                    let hex: Vec<String> =
                                        last.iter().map(|b| format!("{b:02X}")).collect();
                                    let _g = out.lock().unwrap();
                                    println!("{:6.1}s {tag} [{n}] {}", start.elapsed().as_secs_f32(), hex.join(" "));
                                }
                            }
                            Err(e) => {
                                let _g = out.lock().unwrap();
                                println!("[erro] {tag}: {e}");
                                break;
                            }
                        }
                    }
                }));
            }
            Err(e) => println!("[falhou abrir] {tag}: {e}"),
        }
    }
    println!("--- escutando por {secs}s ---");
    for h in handles {
        let _ = h.join();
    }
    println!("--- fim ---");
}
