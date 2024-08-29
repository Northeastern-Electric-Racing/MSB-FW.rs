use defmt::{unwrap, warn};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::{
    adc::RingBufferedAdc,
    can::{Frame, StandardId},
    peripherals::ADC1,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::{Delay, Timer};
use sht3x_ner::Repeatability;

use crate::SharedI2c3;

#[embassy_executor::task]
pub async fn temperature_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let mut sht30 = sht3x_ner::Sht3x::new(i2c_dev, sht3x_ner::Address::High);

    loop {
        Timer::after_millis(500).await;
        let Ok(res) = sht30
            .measure(
                sht3x_ner::ClockStretch::Disabled,
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

        let frame = Frame::new_data(unwrap!(StandardId::new(0x602)), &bits)
            .expect("Could not create frame");
        can_send.send(frame).await;
    }
}

#[embassy_executor::task]
pub async fn imu_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let Ok(mut lsm6dso) = lsm6dso_ner::Lsm6dso::new(i2c_dev, 0x6A).await else {
        warn!("Could not initialize lsm6dso!");
        return;
    };

    loop {
        Timer::after_millis(500).await;
        let Ok(accel) = lsm6dso.read_accelerometer().await else {
            warn!("Could not read lsm6dso accel");
            continue;
        };
        let Ok(gyro) = lsm6dso.read_gyro().await else {
            warn!("Could not read lsm6dso gyro");
            continue;
        };

        let mut accel_bits: [u8; 6] = [0; 6];
        accel_bits[0..2].copy_from_slice(&(((accel.0 * 1000.0) as i16).to_be_bytes()));
        accel_bits[2..4].copy_from_slice(&(((accel.1 * 1000.0) as i16).to_be_bytes()));
        accel_bits[4..].copy_from_slice(&(((accel.2 * 1000.0) as i16).to_be_bytes()));

        let accel_frame = Frame::new_data(unwrap!(StandardId::new(0x603)), &accel_bits)
            .expect("Could not create frame");

        let mut gyro_bits: [u8; 6] = [0; 6];
        gyro_bits[0..2].copy_from_slice(&(((gyro.0 * 1000.0) as i16).to_be_bytes()));
        gyro_bits[2..4].copy_from_slice(&(((gyro.1 * 1000.0) as i16).to_be_bytes()));
        gyro_bits[4..].copy_from_slice(&(((gyro.2 * 1000.0) as i16).to_be_bytes()));

        let gyro_frame = Frame::new_data(unwrap!(StandardId::new(0x604)), &gyro_bits)
            .expect("Could not create frame");

        can_send.send(accel_frame).await;
        can_send.send(gyro_frame).await;
    }
}

#[embassy_executor::task]
pub async fn tof_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let Ok(mut vl6180x) = vl6180x_ner::VL6180X::new(i2c_dev).await else {
        warn!("Could not initialize lsm6dso!");
        return;
    };

    loop {
        let Ok(rng) = vl6180x.poll_range_mm_single_blocking().await else {
            warn!("Failed to get measurement!");
            continue;
        };
        let range_bits = rng.to_be_bytes();
        can_send
            .send(unwrap!(Frame::new_standard(0x607, &range_bits)))
            .await;

        Timer::after_millis(500).await;
    }
}

#[embassy_executor::task]
pub async fn adc1_reader(
    mut adc1: RingBufferedAdc<'static, ADC1>,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let mut measurements: [u16; 60] = [0u16; 120 / 2];

    loop {
        match adc1.read(&mut measurements).await {
            Ok(_) => {
                adc1.teardown_adc();
                // TODO transform measurements
                let mut strain_bits: [u8; 4] = [0; 4];
                strain_bits[0..2].copy_from_slice(&measurements[1].to_be_bytes());
                strain_bits[2..4].copy_from_slice(&measurements[2].to_be_bytes());
                can_send
                    .send(unwrap!(Frame::new_standard(
                        0x606,
                        &measurements[0].to_be_bytes()
                    )))
                    .await;
                can_send
                    .send(unwrap!(Frame::new_standard(0x605, &strain_bits)))
                    .await;
            }
            Err(_) => {
                warn!("DMA overrun");
                continue;
            }
        }
        Timer::after_millis(250).await;
    }
}
