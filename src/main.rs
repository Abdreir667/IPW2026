#![no_std]
#![no_main]

use core::{cell::RefCell, fmt::Write};
use embassy_stm32::adc::{self, Averaging, Resolution, SampleTime};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Pull;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embedded_graphics::text::renderer::CharacterStyle;
use libm::*;

use defmt::{debug, info};
use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::blocking::spi::SpiDeviceWithConfig;
use embassy_executor::Spawner;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::{
	Config,
	gpio::{Level, Output, OutputType, Speed},
	rcc::{Pll, PllDiv, PllMul, PllPreDiv, PllSource, Sysclk, VoltageScale, mux},
	spi::{self, Spi},
	time::{self, Hertz},
};
use embassy_sync::blocking_mutex::{Mutex, raw::NoopRawMutex};
use embassy_time::{Delay, Instant, Timer};
use embedded_graphics::{
	Drawable,
	draw_target::DrawTarget,
	mono_font::{MonoTextStyle, ascii::FONT_6X10},
	pixelcolor::Rgb565,
	prelude::{Point, RgbColor},
	text::Text,
};
use embedded_hal::spi::SpiDevice;
use mipidsi::{
	interface::SpiInterface,
	models::ST7735s,
	options::{Orientation, Rotation},
};

use embassy_stm32::Peri;
use embassy_stm32::peripherals::{ADC1, PA0, PA4};

use panic_probe as _;

const REG_ADDR: u8 = 0x3B;
const WRITE_ADDR_PWR: u8 = 0x6B;
const WRITE_CONFIG: u8 = 0x1C;
const SCALE_F: f32 = 16384.0;
const GYRO_SCALE: f32 = 131.0;

static CHANNEL: Channel<ThreadModeRawMutex, f32, 64> = Channel::new();
static CHANNEL2: Channel<ThreadModeRawMutex, f32, 64> = Channel::new();
static BUT: Channel<ThreadModeRawMutex, bool, 64> = Channel::new();
static JOYSTICK: Channel<ThreadModeRawMutex, f32, 64> = Channel::new();
static JOYSTICK2: Channel<ThreadModeRawMutex, f32, 64> = Channel::new();

fn combine_bytes(first: u8, second: u8) -> i16 {
	((first as i16) << 8) | (second as i16)
}

fn display(
	roll: f32,
	pitch: f32,
	yaw: f32,
	screen: &mut impl DrawTarget<Color = Rgb565>,
	style: MonoTextStyle<'_, Rgb565>,
) {
	let mut acceleration_buf = heapless::String::<120>::new();
	core::write!(
		&mut acceleration_buf,
		"Roll:\n [{}]\n\nPitch:\n [{}]\n\nYaw:\n [{}]",
		trans_to_disp(-roll),
		trans_to_disp(pitch),
		trans_to_disp(-yaw)
	)
	.unwrap();

	let _ = Text::new(&acceleration_buf, Point::new(0, 20), style).draw(screen);
}

#[embassy_executor::task]
async fn reset_btn(mut btn: ExtiInput<'static>) {
	loop {
		btn.wait_for_falling_edge().await;
		BUT.send(true).await;
	}
}

#[embassy_executor::task]
async fn control_joystick(
	mut adc: adc::Adc<'static, ADC1>,
	mut pa_pin_x: Peri<'static, PA0>,
	mut pa_pin_y: Peri<'static, PA4>,
) {
	adc.set_resolution(Resolution::BITS14);
	adc.set_averaging(Averaging::Samples1024);
	adc.set_sample_time(SampleTime::CYCLES160_5);

	const MAX_VAL: u32 = adc::resolution_to_max_count(Resolution::BITS14);

	loop {
		let val_x = adc.blocking_read(&mut pa_pin_x);
		let perc_x = (val_x as u32 * 100 / MAX_VAL) as u8;
		Timer::after_millis(10).await;
		let val_y = adc.blocking_read(&mut pa_pin_y);
		let perc_y = (val_y as u32 * 100 / MAX_VAL) as u8;


		if perc_x >= 60 {
			JOYSTICK.send(1.0).await;
		} else if perc_x <= 40 {
			JOYSTICK.send(-1.0).await;
		}

		if perc_y >= 60 {
			JOYSTICK2.send(1.0).await;
		} else if perc_y <= 40 {
			JOYSTICK2.send(-1.0).await;
		}

		Timer::after_millis(200).await;
	}

}

#[embassy_executor::task]
async fn move_motor(_old_yaw: f64, mut pwm: SimplePwm<'static, embassy_stm32::peripherals::TIM1>) {
	let mut ch1 = pwm.ch1();
	ch1.enable();
	loop {
		let yaw = CHANNEL.receive().await;
		ch1.set_duty_cycle_fraction(transform(yaw) as u16, 10_000);
	}
}

#[embassy_executor::task]
async fn move_motor2(
	_old_yaw2: f64,
	mut pwm: SimplePwm<'static, embassy_stm32::peripherals::TIM3>,
) {
	let mut ch1 = pwm.ch2();
	ch1.enable();
	loop {
		let yaw = CHANNEL2.receive().await;
		ch1.set_duty_cycle_fraction(transform(yaw) as u16, 10_000);
	}
}

fn trans_to_disp(angle: f32) -> &'static str {
	if angle < -80.0 {
		"#                "
	} else if angle < -70.0 {
		" #               "
	} else if angle < -60.0 {
		"  #              "
	} else if angle < -50.0 {
		"   #             "
	} else if angle < -40.0 {
		"    #            "
	} else if angle < -30.0 {
		"     #           "
	} else if angle < -20.0 {
		"      #          "
	} else if angle < -10.0 {
		"       #         "
	} else if angle < 0.0 {
		"        #        "
	} else if angle < 10.0 {
		"         #       "
	} else if angle < 20.0 {
		"          #      "
	} else if angle < 30.0 {
		"           #     "
	} else if angle < 40.0 {
		"            #    "
	} else if angle < 50.0 {
		"             #   "
	} else if angle < 60.0 {
		"              #  "
	} else if angle < 70.0 {
		"               # "
	} else {
		"                #"
	}
}

fn transform(x: f32) -> f32 {
	x * 50.0 / 9.0 + 750.0
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
	let mut config = Config::default();


	config.rcc.hsi = true;
	config.rcc.pll1 = Some(Pll {
		source: PllSource::HSI,
		prediv: PllPreDiv::DIV1,
		mul: PllMul::MUL10,
		divp: None,
		divq: None,
		divr: Some(PllDiv::DIV1),
	});
	config.rcc.sys = Sysclk::PLL1_R;
	config.rcc.voltage_range = VoltageScale::RANGE1;
	config.rcc.mux.iclksel = mux::Iclksel::HSI48;
	config.rcc.mux.adcdacsel = mux::Adcdacsel::HSI;

	let p = embassy_stm32::init(config);

	let mut warning_pin = Output::new(p.PB6, Level::Low, Speed::Medium);
	let mut good_pin = Output::new(p.PB7, Level::High, Speed::Medium);
	let pwm_pin = PwmPin::new(p.PA8, OutputType::PushPull);

	let mut il_butoaine = ExtiInput::new(p.PA3, p.EXTI3, Pull::None);

	let mut pwm = SimplePwm::new(
		p.TIM1,
		Some(pwm_pin),
		None,
		None,
		None,
		time::hz(50),
		Default::default(),
	);

	let pwm_pin2 = PwmPin::new(p.PC7, OutputType::PushPull);

	let mut pwm2 = SimplePwm::new(
		p.TIM3,
		None,
		Some(pwm_pin2),
		None,
		None,
		time::hz(50),
		Default::default(),
	);

	spawner.spawn(move_motor(0.0, pwm)).unwrap();
	spawner.spawn(move_motor2(0.0, pwm2)).unwrap();
	spawner.spawn(reset_btn(il_butoaine)).unwrap();
	spawner
		.spawn(control_joystick(adc::Adc::new(p.ADC1), p.PA0, p.PA4))
		.unwrap();

	const MIN_PERIOD_US: u32 = 500;
	const MAX_PERIOD_US: u32 = 2500;
	const PERIOD_US: u32 = 20000;

	let mut roll: f32 = 0.0;
	let mut yaw: f32 = 0.0;
	let mut pitch: f32 = 0.0;

	info!("Device started");

	let spi_bus_raw = Spi::new_blocking(p.SPI1, p.PA5, p.PA7, p.PA6, spi::Config::default());
	let spi_bus: Mutex<NoopRawMutex, _> = Mutex::new(RefCell::new(spi_bus_raw));

	let sensor_cs = Output::new(p.PC9, Level::High, Speed::Low);
	let mut sensor_spi_config = spi::Config::default();
	sensor_spi_config.frequency = Hertz(1_000_000);
	let mut sensor_spi = SpiDeviceWithConfig::new(&spi_bus, sensor_cs, sensor_spi_config);

	let screen_cs = Output::new(p.PB5, Level::High, Speed::Low);
	let screen_rst = Output::new(p.PC8, Level::Low, Speed::Low);

	let screen_dc = Output::new(p.PB3, Level::Low, Speed::Low);

	let mut screen_spi_config = spi::Config::default();
	screen_spi_config.frequency = Hertz(10_000_000);
	let display_spi = SpiDeviceWithConfig::new(&spi_bus, screen_cs, screen_spi_config);

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

	let mut rx_dummy = [0x00u8; 2];
	let tx_reset = [!(1 << 7) & WRITE_ADDR_PWR, 0x80];
	sensor_spi.transfer(&mut rx_dummy, &tx_reset).unwrap();
	Timer::after_millis(100).await;

	let tx_disable_i2c = [!(1 << 7) & 0x6A, 0x10];
	sensor_spi.transfer(&mut rx_dummy, &tx_disable_i2c).unwrap();
	Timer::after_millis(10).await;

	// MPU Init
	let tx_pwr = [!(1 << 7) & WRITE_ADDR_PWR, 0x00];
	let mut rx_dummy = [0u8; 2];
	sensor_spi.transfer(&mut rx_dummy, &tx_pwr).unwrap();
	Timer::after_millis(10).await;

	let tx_cfg = [!(1 << 7) & WRITE_CONFIG, 0x00];
	sensor_spi.transfer(&mut rx_dummy, &tx_cfg).unwrap();

	info!("Calibrating Gyro Z... Do not move board!");
	let mut gyro_z_sum: f32 = 0.0;
	let samples = 200;
	for _ in 0..samples {
		let tx_buf = [(1 << 7) | 0x47, 0x00, 0x00];
		let mut rx_buf = [0u8; 3];
		sensor_spi.transfer(&mut rx_buf, &tx_buf).unwrap();

		let raw_gz = combine_bytes(rx_buf[1], rx_buf[2]) as f32;
		gyro_z_sum += raw_gz;
		Timer::after_millis(5).await;
	}
	let gyro_z_offset = gyro_z_sum / (samples as f32);
	let message= "Calibrating!";

	let _ = Text::new(&message, Point::new(0, 20), style).draw(&mut screen);

	Timer::after_secs(1).await;
	screen.clear(Rgb565::BLACK).unwrap();

	warning_pin.set_high();
	good_pin.set_low();
	let mut yaw: f32 = 0.0;
	let mut last_time = Instant::now();
	let mut old_roll = 0.0;
	let mut rolly = 0.0;

	let mut last_display_time = Instant::now();

	loop {

		let now = Instant::now();

		if now.duration_since(last_display_time).as_millis() > 1000 {
			display(roll, pitch, yaw, &mut screen, style);
			CHANNEL2.send(roll).await;
			last_display_time = now;
		}

		let tx_buf = [
			(1 << 7) | REG_ADDR,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
			0,
		];
		let mut rx_buf = [0u8; 15];
		sensor_spi.transfer(&mut rx_buf, &tx_buf).unwrap();

		let res_x = combine_bytes(rx_buf[1], rx_buf[2]);
		let res_y = combine_bytes(rx_buf[3], rx_buf[4]);
		let res_z = combine_bytes(rx_buf[5], rx_buf[6]);
		let raw_gz = combine_bytes(rx_buf[13], rx_buf[14]);

		let acc_x = (res_x as f32) / SCALE_F;
		let acc_y = (res_y as f32) / SCALE_F;
		let acc_z = (res_z as f32) / SCALE_F;


		pitch = atan2f(acc_y, sqrtf(acc_x * acc_x + acc_z * acc_z)).to_degrees();
		roll = atan2f(-acc_x, sqrtf(acc_y * acc_y + acc_z * acc_z)).to_degrees();
		roll += rolly;
		let delta_t = now.duration_since(last_time).as_micros() as f32 / 1_000_000.0;
		last_time = now;

		let gyro_z_dps = ((raw_gz as f32) - gyro_z_offset) / GYRO_SCALE;
		let filtered_gz = if gyro_z_dps.abs() < 0.3 {
			0.0
		} else {
			gyro_z_dps
		};

		let old_yaw = yaw;
		yaw += filtered_gz * delta_t;

		if (yaw - old_yaw).abs() < 2.0 {
			CHANNEL.send(-yaw).await;
		}

		if (roll - old_roll).abs() < 2.0 {
			CHANNEL2.send(roll).await;
		}

		let value = BUT.try_receive();
		match value {
			Ok(_v) => {
				yaw = 0.0;
				rolly = 0.0;
				roll = 0.0;
				info!("Neutralize\n\n\n\n");
			}
			_ => {}
		}

		let val = JOYSTICK.try_receive();
		match val {
			Ok(v) => {
				info!("Adjusting yaw");
				yaw += 5.0 * v;
			}
			Err(_) => {}
		}

		let val2 = JOYSTICK2.try_receive();
		match val2 {
			Ok(v) => {
				info!("Adjusting roll");
				rolly += 5.0 * v;
			}
			Err(_) => {}
		}

		old_roll = roll;
	}
}
