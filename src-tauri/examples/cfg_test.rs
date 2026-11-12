// Teste do modulo de configuracao do mouse. Faz leituras e gravacoes REVERSIVEIS:
// cada alteracao e conferida por leitura e o valor original e restaurado no fim.
// Uso: cargo run --example cfg_test
use unison_lib::mouse_cfg as cfg;

fn show(c: &cfg::MouseConfig) {
    println!(
        "  via={} polling={}Hz estagios={} atual={}",
        c.via, c.report_rate, c.stage_count, c.current_stage + 1
    );
    for (i, s) in c.stages.iter().enumerate().take(c.stage_count as usize) {
        println!("   {}: {:>5} DPI  cor #{:02X}{:02X}{:02X}", i + 1, s.dpi, s.color[0], s.color[1], s.color[2]);
    }
    println!(
        "  luz DPI: efeito={} brilho={} velocidade={}",
        c.dpi_light.effect, c.dpi_light.brightness, c.dpi_light.speed
    );
}

fn main() {
    println!("== Leitura inicial");
    let orig = cfg::read_config().expect("leitura");
    show(&orig);

    if std::env::args().nth(1).as_deref() != Some("write") {
        println!("\n(so leitura; rode com o argumento 'write' para testar as gravacoes reversiveis)");
        return;
    }

    println!("\n== Polling: regravar o mesmo valor ({} Hz)", orig.report_rate);
    cfg::set_report_rate(orig.report_rate).expect("polling igual");
    println!("  ok (conferido por leitura)");

    let other = if orig.report_rate == 500 { 250 } else { 500 };
    println!("\n== Polling: {} -> {} Hz -> {} Hz", orig.report_rate, other, orig.report_rate);
    cfg::set_report_rate(other).expect("polling novo");
    println!("  agora: {} Hz", cfg::read_config().unwrap().report_rate);
    cfg::set_report_rate(orig.report_rate).expect("restaurar polling");
    println!("  restaurado: {} Hz", cfg::read_config().unwrap().report_rate);

    // estagio com DPI alto (usa o byte de extensao) e um normal
    let last = (orig.stage_count - 1) as usize;
    for idx in [1usize, last] {
        let s = orig.stages[idx].clone();
        let tmp = if s.dpi == 1200 { 1250 } else { 1200 };
        println!("\n== Estagio {}: {} -> {} -> {} DPI", idx + 1, s.dpi, tmp, s.dpi);
        cfg::set_stage(idx as u8, tmp, s.color).expect("dpi novo");
        println!("  agora: {} DPI", cfg::read_config().unwrap().stages[idx].dpi);
        cfg::set_stage(idx as u8, s.dpi, s.color).expect("restaurar dpi");
        println!("  restaurado: {} DPI", cfg::read_config().unwrap().stages[idx].dpi);
    }

    println!("\n== Leitura final (deve ser igual a inicial)");
    let fin = cfg::read_config().unwrap();
    show(&fin);
    let same = fin.report_rate == orig.report_rate
        && fin.current_stage == orig.current_stage
        && fin.stages.iter().zip(&orig.stages).all(|(a, b)| a.dpi == b.dpi && a.color == b.color);
    println!("\nRESULTADO: {}", if same { "configuração idêntica à original" } else { "DIFERENTE da original!" });
}
