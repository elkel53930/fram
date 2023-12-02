use anyhow::Ok;
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::AnyIOPin;
use esp_idf_hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::units::Hertz;

const I2C_ADDRESS: u8 = 0x50;

// I2Cドライバのインスタンス
static mut I2C: Option<I2cDriver<'static>> = None;

// FRAMへの書き込み位置
static mut CURSOR: u16 = 0;

// I2Cの初期化
fn i2c_master_init<'d>(
    i2c: impl Peripheral<P = impl I2c> + 'd,
    sda: AnyIOPin,
    scl: AnyIOPin,
    baudrate: Hertz,
) -> anyhow::Result<I2cDriver<'d>> {
    let config = I2cConfig::new().baudrate(baudrate);
    let driver = I2cDriver::new(i2c, sda, scl, &config)?;
    Ok(driver)
}

fn write_30_byte(adrs: u16, data: &[u8]) -> anyhow::Result<()> {
    let mut buffer: [u8; 32] = [0; 32];

    // 最初の2byteがアドレスなので、一度に書き込めるデータは30byteまで
    buffer[0] = (adrs >> 8) as u8;
    buffer[1] = adrs as u8;
    buffer[2..2 + data.len()].copy_from_slice(data);
    unsafe {
        I2C.as_mut().unwrap().write(I2C_ADDRESS, &buffer, BLOCK)?;
    }
    Ok(())
}

fn write_fram(adrs: u16, data: &[u8]) -> anyhow::Result<()> {
    let mut i = 0;

    // 30byteずつ書き込む
    while i < data.len() {
        let mut j = i + 30;
        if j > data.len() {
            j = data.len();
        }
        write_30_byte(adrs + i as u16, &data[i..j])?;
        i += 30;
    }
    Ok(())
}

pub fn read_fram(adrs: u16, data: &mut [u8]) -> anyhow::Result<()> {
    let buffer: [u8; 2] = [(adrs >> 8) as u8, adrs as u8];
    unsafe {
        // アドレスを書き込んでから読み込む
        I2C.as_mut().unwrap().write(I2C_ADDRESS, &buffer, BLOCK)?;
        I2C.as_mut().unwrap().read(I2C_ADDRESS, data, BLOCK)?;
    }
    Ok(())
}

pub fn init(peripherals: &mut Peripherals) -> anyhow::Result<()> {
    unsafe {
        let i2c = i2c_master_init(
            peripherals.i2c0.clone_unchecked(),
            peripherals.pins.gpio18.clone_unchecked().into(),
            peripherals.pins.gpio17.clone_unchecked().into(),
            1000.kHz().into(),
        )?;
        I2C = Some(i2c);
    };
    Ok(())
}

// println!やprint!を同じように使えるマクロ

use core::fmt::{self, Write};

pub fn fram_print(args: fmt::Arguments) {
    let mut writer = FramWriter {};
    writer.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! fprint {
    ($($arg:tt)*) => ($crate::fram_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! fprintln {
    ($fmt:expr) => (fprint!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (fprint!(concat!($fmt, "\n"), $($arg)*));
}

struct FramWriter;

fn write(s: &str) -> fmt::Result {
    // 文字列をFRAMに書き込む
    write_fram(unsafe { CURSOR }, s.as_bytes()).unwrap();

    // 書き込んだ分だけカーソルを進める
    unsafe {
        CURSOR += s.len() as u16;
        while CURSOR > 0x2000 {
            CURSOR -= 0x2000;
        }
    }

    // 終端文字を書き込む
    write_fram(unsafe { CURSOR }, b"\0").unwrap();

    core::result::Result::Ok(())
}

impl Write for FramWriter {
    fn write_str(&mut self, s: &str) -> Result<(), std::fmt::Error> {
        write(&s)
    }
}

// FRAMに書き込まれたログを表示
pub fn show_log() {
    let mut buffer: [u8; 32] = [0; 32];
    let mut adrs = 0;
    let mut flag = true;

    println!("\n\nLog - - - - - - - - - - - - - - -");

    while flag {
        let mut size = 0;
        read_fram(adrs, &mut buffer).unwrap();
        adrs += buffer.len() as u16;
        for b in buffer {
            size += 1;
            if b == 0 {
                flag = false;
                break;
            }
        }
        let s = std::str::from_utf8(&buffer[0..size]).unwrap();
        print!("{}", s);
    }

    println!("- - - - - - - - - - - - - - - - -");
}

// FRAMを使ったpanicハンドラ
use std::panic::{self, PanicInfo};

fn fram_panic_handler(info: &PanicInfo) {
    if let Some(location) = info.location() {
        fprintln!(
            "Panic occurred in file '{}' at line {}",
            location.file(),
            location.line()
        );
        println!(
            "Panic occurred in file '{}' at line {}",
            location.file(),
            location.line()
        );
    } else {
        fprintln!("Panic occurred but can't get location information...");
        println!("Panic occurred but can't get location information...");
    }
    fprintln!("{}", info);
    println!("{}", info);

    // panicが発生したら再起動せずに停止
    loop {
        // ウォッチドッグタイマによるリセットを防ぐ
        esp_idf_hal::delay::FreeRtos::delay_ms(1000);
    }
}

pub fn set_panic_handler() {
    panic::set_hook(Box::new(fram_panic_handler));
}

use log::{Level, Metadata, Record};
pub struct FramLogger;

impl log::Log for FramLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            fprintln!("{} - {}", record.level(), record.args());
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn set_log(log_level: log::LevelFilter) {
    log::set_boxed_logger(Box::new(FramLogger))
        .map(|()| log::set_max_level(log_level))
        .unwrap();
}
