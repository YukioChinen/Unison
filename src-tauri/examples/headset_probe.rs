// Teste sem GUI do modulo do headset: cargo run --example headset_probe
// Fecha o ATK HUB antes (ele segura a COM3). Ctrl+C para sair.
fn main() {
    println!("Monitorando o headset...");
    unison_lib::headset::run(None, |s| {
        println!(
            "{:<10} ligado={} carregando={} cru={:?} exibido={:?}{} taxa={:.2}%/min",
            s.status,
            s.headset_on,
            s.charging,
            s.raw_percent,
            s.percent.map(|p| p.round()),
            if s.estimated { " (estimado)" } else { "" },
            s.charge_rate
        );
    });
}
