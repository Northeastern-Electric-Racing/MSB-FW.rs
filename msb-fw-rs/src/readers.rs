use defmt::{unwrap, warn};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::can::bxcan::{Frame, StandardId};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::{Delay, Timer};
use sht3x::Repeatability;

use crate::SharedI2c3;

#[embassy_executor::task]
pub async fn temperature_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let mut sht30 = sht3x::Sht3x::new(i2c_dev, sht3x::Address::High);

    loop {
        Timer::after_millis(500).await;
        let Ok(res) = sht30
            .measure(
                sht3x::ClockStretch::Disabled,
                Repeatability::High,
                &mut Delay,
            )
            .await
        else {
            warn!("Could not get temperature");
            continue;
        };
        let temp: [u8; 2] = (res.temperature as i16).to_be_bytes();
        let humidity: [u8; 2] = (res.humidity).to_be_bytes();
        let mut bits: [u8; 4] = [0; 4];
        bits[..2].copy_from_slice(&temp);
        bits[2..].copy_from_slice(&humidity);

        let frame = Frame::new_data(unwrap!(StandardId::new(0x602)), bits);
        can_send.send(frame).await;
    }
}
