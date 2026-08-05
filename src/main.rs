#![no_main]
#![no_std]

use defmt::info;
use embassy_stm32::spi::*;
use embassy_stm32::timer::low_level::OutputPolarity;
use embassy_stm32::*;
use libm::{atan2f, sqrtf, sin};

use core::sync::atomic::{AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Pull};
use embassy_stm32::gpio::{Level, Output, OutputType, Speed};
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};
// use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{AnyPin, Pin};

use embassy_stm32::peripherals::*;
use embassy_stm32::timer::*;
use embassy_stm32::timer::simple_pwm::*;
use embassy_stm32::time::{hz, khz};


const REG_ADDR:u8 = 0x3B;
const WRITE_ADDR_PWR:u8 = 0x6B;
const WRITE_CONFIG:u8 = 0x1C;
const SCALE_F: u16 = 16_384;
const G: f32 = 9.80665;

fn combine_bytes(first: u8, second: u8) -> i16 {
    let mut res: i16;
    res = first as i16;
    res <<= 8;
    res |= second as i16;
    res
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {

    let p = embassy_stm32::init(Default::default());
    let mut config = spi::Config::default();
    config.frequency = hz(1_000_000);

    let miso = p.PA6;
    let mosi = p.PA7;
    let clk = p.PA5;

    let mut spi = Spi::new(p.SPI1, clk, mosi, miso, p.GPDMA1_CH0, p.GPDMA1_CH1, config);

    // make sure to actually choose a pin
    let mut cs = Output::new(p.PC9, Level::High, Speed::Low);

    cs.set_low();
    let tx_buf = [!(1 << 7) & WRITE_ADDR_PWR, 0x00]; // value_to_write is to be replaced with the 8-bit value that we want to write to this register
    let mut rx_buf = [0u8; 2]; // we are not expecting any relevant information to be received, but we still need to receive dummy values anyway
    spi.transfer(&mut rx_buf, &tx_buf).await.unwrap();
    cs.set_high();

    Timer::after_millis(10).await;

    cs.set_low();
    let tx_buf = [!(1 << 7) & WRITE_CONFIG, 0x00]; // value_to_write is to be replaced with the 8-bit value that we want to write to this register
    let mut rx_buf = [0u8; 2]; // we are not expecting any relevant information to be received, but we still need to receive dummy values anyway
    spi.transfer(&mut rx_buf, &tx_buf).await.unwrap();
    cs.set_high();

    let mut last_v = 0.0;
    let mut v = 0.0;
    let delta_t = 0.1;

    loop {

        cs.set_low();
        let tx_buf = [(1 << 7) | REG_ADDR, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut rx_buf = [0u8; 7]; // three receive values instead of two
        spi.transfer(&mut rx_buf, &tx_buf).await.unwrap();
        cs.set_high();


        let register_value = rx_buf[1]; // the second byte in the buffer will be the received register value (REG_ADDR)
        let register_value_next = rx_buf[2]; // the third byte in the buffer will be the next received register value (REG_ADDR+1)
        let regy_h = rx_buf[3];
        let regy_l = rx_buf[4];
        let regz_h = rx_buf[5];
        let regz_l = rx_buf[6];


        let res_x = combine_bytes(register_value, register_value_next);
        let res_y = combine_bytes(regy_h, regy_l);
        let res_z = combine_bytes(regz_h, regz_l);

        // info!("Res x: {}", (res_x as f32) / (SCALE_F as f32));
        // info!("Res y: {}", (res_y as f32) / (SCALE_F as f32));
        // info!("Res z: {}", (res_z as f32) / (SCALE_F as f32));
        let acc_x = (res_x as f32) / (SCALE_F as f32);
        let acc_y = (res_y as f32) / (SCALE_F as f32);
        let acc_z = (res_z as f32) / (SCALE_F as f32);

        let pitch = atan2f(acc_y, sqrtf(acc_x * acc_x + acc_z * acc_z)).to_degrees();
        let roll = atan2f(-acc_x, sqrtf(acc_y * acc_y + acc_z * acc_z)).to_degrees();
        
        // let a_forward = acc_x + G * sin(pitch as f64) as f32;

        // v = last_v + a_forward * delta_t;
        // last_v = v;

        info!("Pitch {} Roll {}", pitch, roll);
        info!("Acc {}", v);


        

        // cs.set_low();
        embassy_time::Timer::after_millis(200).await;

    }
    // loop {};
}
