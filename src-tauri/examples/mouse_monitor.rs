// Monitor sem GUI do modulo do mouse: cargo run --example mouse_monitor
fn main() {
    println!("Monitorando o mouse...");
    unison_lib::mouse::run(|s| {
        println!(
            "{:<11} via={} disponivel={} bateria={:?}% flag[7]={:?} carregando={} tensao={:?}mV",
            s.status,
            if s.wired { "cabo " } else { "dongle" },
            s.available,
            s.percent,
            s.power_flag,
            s.charging,
            s.voltage_mv
        );
    });
}
