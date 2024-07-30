use embassy_stm32::gpio::Output;
use embassy_time::Timer;

use crate::DeviceLocation;

#[embassy_executor::task]
pub async fn control_leds(
    mut led1: Output<'static>,
    mut led2: Output<'static>,
    device_loc: DeviceLocation,
) {
    loop {
        Timer::after_secs(2).await;
        match device_loc {
            DeviceLocation::FrontLeft => {
                led1.set_high();
                led2.set_high();
            }
            DeviceLocation::BackLeft => {
                led1.set_low();
                led2.set_high();
            }
            DeviceLocation::BackRight => {
                led1.set_low();
                led2.set_low();
            }
            DeviceLocation::FrontRight => {
                led1.set_high();
                led2.set_low();
            }
        }
    }
}
