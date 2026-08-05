#![no_std]
#![no_main]

use core::{cell::RefCell, fmt::Write};

use defmt::{debug, info, warn};
use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::blocking::spi::SpiDeviceWithConfig;
use embassy_executor::Spawner;
use embassy_stm32::{
    Config,
    gpio::{Level, Output, Speed},
    rcc::{Pll, PllDiv, PllMul, PllPreDiv, PllSource, Sysclk, VoltageScale, mux},
    spi::{self, Spi},
    time::Hertz,
};
use embassy_sync::blocking_mutex::{Mutex, raw::NoopRawMutex};
use embassy_time::{Delay, Timer};
use embedded_graphics::{
    Drawable,
    draw_target::DrawTarget,
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::{Point, RgbColor},
    text::{Text, renderer::CharacterStyle},
};
use mipidsi::{
    interface::SpiInterface,
    models::ST7735s,
    options::{Orientation, Rotation},
};

use panic_probe as _;

fn trans_to_disp(angle : f32) -> &'static str {

    if angle < -80.0 {
        return "#                ";
    } else if angle < -70.0 {
        return " #               ";
    } else if angle < -60.0 {
        return "  #              ";
    } else if angle < -50.0 {
        return "   #             ";
    } else if angle < -40.0 {
        return "    #            ";
    } else if angle < -30.0 {
        return "     #           ";
    } else if angle < -20.0 {
        return "      #          ";
    } else if angle < -10.0 {
        return "       #         ";
    } else if angle < 0.0 {
        return "        #        ";
    } else if angle < 10.0 {
        return "         #       ";
    } else if angle < 20.0 {
        return "          #      ";
    } else if angle < 30.0 {
        return "           #     ";
    } else if angle < 40.0 {
        return "            #    ";
    } else if angle < 50.0 {
        return "             #   ";
    } else if angle < 60.0 {
        return "              #  ";
    } else if angle < 70.0 {
        return "               # ";
    } else {
        return "                #";
    }

}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {

    let mut config = Config::default();
    config.rcc.hsi = true;
    config.rcc.pll1 = Some(Pll {
        source: PllSource::HSI, // 16 MHz
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL10,
        divp: None,
        divq: None,
        divr: Some(PllDiv::DIV1), // 160 MHz
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.voltage_range = VoltageScale::RANGE1;
    config.rcc.mux.iclksel = mux::Iclksel::HSI48; // USB uses ICLK

    let peripherals = embassy_stm32::init(config);
    info!("Device started");

    let screen_rst = Output::new(peripherals.PC8, Level::Low, Speed::Low);
    let screen_dc = Output::new(peripherals.PB3, Level::Low, Speed::Low);

    // SPI1 is exposed by the Arduino header using pins:
    // - MISO - D12 (PA6)
    // - MOSI - D11 (PA7)
    // - CLK - D13 (PA5)
    //
    // We need a blocking SPI as the `mipidsi` display drivers require a blocking SPI device.
    let spi = Spi::new_blocking(
        peripherals.SPI1,
        peripherals.PA5,
        peripherals.PA7,
        peripherals.PA6,
        spi::Config::default(),
    );
    let spi_bus_mutex: Mutex<NoopRawMutex, _> = Mutex::new(RefCell::new(spi));
    let mut screen_spi_config = spi::Config::default();
    screen_spi_config.frequency = Hertz(10_000_000);
    let screen_cs = Output::new(peripherals.PB5, Level::High, Speed::Low);
    let display_spi = SpiDeviceWithConfig::new(&spi_bus_mutex, screen_cs, screen_spi_config);
    let mut screen_buffer = [0; 4096];
    let di = SpiInterface::new(display_spi, screen_dc, &mut screen_buffer);
    let mut screen = mipidsi::Builder::new(ST7735s, di)
        .reset_pin(screen_rst)
        .orientation(Orientation::new().rotate(Rotation::Deg180))
        .init(&mut Delay)
        .unwrap();

    screen.clear(Rgb565::BLACK).unwrap();
    let mut style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    style.set_background_color(Some(Rgb565::BLACK));

    loop {

        let mut acceleration_buf = heapless::String::<100>::new();
        core::write!(
            &mut acceleration_buf,
            "Roll:\n [{}]\n\nPitch:\n [{}]\n\nYaw:\n [{}]", trans_to_disp(90.0), trans_to_disp(-50.6), trans_to_disp(-80.2)
        )
        .unwrap();
        Text::new(&acceleration_buf, Point::new(0, 20), style)
            .draw(&mut screen)
            .unwrap();
        debug!(
            "Roll, Pitch:{}, {}\n", 12, 14
        );
        Timer::after_millis(100).await;
    }

}