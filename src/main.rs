use esp_idf_hal::peripherals::Peripherals;

#[macro_use] // マクロを使うためのおまじない
pub mod fram_logger;
use crate::fram_logger::fram_print;

fn main() {
    esp_idf_svc::sys::link_patches();

    // FRAMとpanicハンドラの初期化
    let mut peripherals = Peripherals::take().unwrap();
    let _ = fram_logger::init(&mut peripherals);
    let _ = fram_logger::set_panic_handler();

    // 前回のログを表示
    fram_logger::show_log();

    // ログを書き込む
    fprintln!("FRAM logger test");

    // 意図的なpanic
    let array: [u8; 3] = [1, 2, 3];

    for i in 0..5 {
        fprintln!("array[{}] = {}", i, array[i]); // i = 3でpanic
        println!("array[{}] = {}", i, array[i]);
    }
}
