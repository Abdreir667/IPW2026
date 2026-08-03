#![no_main]
#![no_std]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let peripherals = embassy_stm32::init(Default::default());

    let mut x = 1;
    loop {
        info!("Hello, embassy-rs! x = {}", x);
        x += 1;
        Timer::after(Duration::from_millis(500)).await;
    }
}
