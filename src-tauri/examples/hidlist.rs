// Checagem rapida sem GUI: cargo run --example hidlist
use hidapi::HidApi;

fn main() {
    let api = HidApi::new().expect("hidapi");
    for d in api.device_list() {
        if d.vendor_id() == 0x3554 || d.vendor_id() == 0x373B {
            println!(
                "{:04X}:{:04X} if={:<2} page=0x{:04X} usage=0x{:04X} {}",
                d.vendor_id(),
                d.product_id(),
                d.interface_number(),
                d.usage_page(),
                d.usage(),
                d.product_string().unwrap_or("")
            );
        }
    }
}
