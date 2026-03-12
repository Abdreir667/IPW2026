#![no_main]
#![no_std]

use defmt::info;
use embassy_executor::Spawner;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let peripherals = embassy_stm32::init(Default::default());

    info!("Hello, embassy-rs!");
    loop {}
}
