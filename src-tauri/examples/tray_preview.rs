// Gera amostras do icone da bandeja em RGBA cru: cargo run --example tray_preview -- <pasta>
fn main() {
    let dir = std::env::args().nth(1).expect("pasta de saida");
    let samples: [(&str, Option<u8>, bool); 6] = [
        ("a_70", Some(70), false),
        ("b_100", Some(100), false),
        ("c_9", Some(9), false),
        ("d_35", Some(35), false),
        ("e_60_chg", Some(60), true),
        ("f_none", None, false),
    ];
    for (name, v, c) in samples {
        let img = unison_lib::tray::icon(v, c);
        std::fs::write(format!("{dir}/{name}.rgba"), img.rgba()).unwrap();
    }
}
