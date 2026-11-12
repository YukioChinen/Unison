// Teste reversivel do indicador de DPI do mouse. Cada passo e conferido por leitura e o
// estado original e restaurado no fim, mesmo se um passo falhar.
// Uso: cargo run --example light_test [segundos_por_passo]
use std::{thread::sleep, time::Duration};
use unison_lib::mouse_cfg as cfg;

fn main() {
    let hold: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let orig = cfg::read_config().expect("leitura inicial");
    let d = orig.dpi_light.clone();
    println!("Original -> indicador DPI: efeito={} brilho={} vel={}", d.effect, d.brightness, d.speed);

    let mut falhas = 0;
    let mut passo = |nome: &str, r: Result<(), String>| {
        match r {
            Ok(()) => println!("[ok]    {nome}"),
            Err(e) => {
                falhas += 1;
                println!("[FALHA] {nome}: {e}");
            }
        }
        sleep(Duration::from_secs(hold));
    };

    passo("indicador DPI: respirando, alto, rapido", cfg::set_dpi_light(2, 255, 5));
    passo("indicador DPI: fixo, alto", cfg::set_dpi_light(1, 255, 3));
    passo("indicador DPI: desligado", cfg::set_dpi_light(0, 255, 3));

    let r = cfg::set_dpi_light(d.effect, d.brightness, d.speed);
    println!("restaurar indicador: {:?}", r);

    let f = cfg::read_config().expect("leitura final").dpi_light;
    let igual = f.effect == d.effect && f.brightness == d.brightness && f.speed == d.speed;
    println!("RESULTADO: {} ({} falha(s))", if igual { "restaurado igual ao original" } else { "DIFERENTE do original!" }, falhas);
}
