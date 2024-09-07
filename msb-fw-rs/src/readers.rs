use defmt::{unwrap, warn};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_stm32::{
    adc::RingBufferedAdc,
    can::{Frame, StandardId},
    peripherals::ADC1,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Sender};
use embassy_time::{Delay, Duration, Timer};
use sht3x_ner::Repeatability;

use crate::SharedI2c3;

const TEMPERATURE_REFRESH_TIME: Duration = Duration::from_millis(500);
const TEMPERATURE_SEND_MSG_ID: StandardId = StandardId::new(0x602).expect("Could not parse ID");

#[embassy_executor::task]
pub async fn temperature_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let mut sht30 = sht3x_ner::Sht3x::new(i2c_dev, sht3x_ner::Address::High);

    loop {
        Timer::after(TEMPERATURE_REFRESH_TIME).await;
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

        let frame =
            Frame::new_data(TEMPERATURE_SEND_MSG_ID, &bits).expect("Could not create frame");
        can_send.send(frame).await;
    }
}

const LSM6DSO_ADDR: u8 = 0x6A;
const IMU_REFRESH_TIME: Duration = Duration::from_millis(500);
const IMU_SEND_MSG_ID: StandardId = StandardId::new(0x603).expect("Could not parse ID");
const GYRO_SEND_MSG_ID: StandardId = StandardId::new(0x604).expect("Could not parse ID");

#[embassy_executor::task]
pub async fn imu_reader(
    i2c: &'static SharedI2c3,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let i2c_dev = I2cDevice::new(i2c);
    let Ok(mut lsm6dso) = lsm6dso_ner::Lsm6dso::new(i2c_dev, LSM6DSO_ADDR).await else {
        warn!("Could not initialize lsm6dso!");
        return;
    };

    let mut accel_bits: [u8; 6] = [0; 6];
    let mut gyro_bits: [u8; 6] = [0; 6];

    loop {
        Timer::after(IMU_REFRESH_TIME).await;
        let Ok(accel) = lsm6dso.read_accelerometer().await else {
            warn!("Could not read lsm6dso accel");
            continue;
        };
        let Ok(gyro) = lsm6dso.read_gyro().await else {
            warn!("Could not read lsm6dso gyro");
            continue;
        };

        accel_bits[0..2].copy_from_slice(&(((accel.0 * 1000.0) as i16).to_be_bytes()));
        accel_bits[2..4].copy_from_slice(&(((accel.1 * 1000.0) as i16).to_be_bytes()));
        accel_bits[4..].copy_from_slice(&(((accel.2 * 1000.0) as i16).to_be_bytes()));

        let accel_frame =
            Frame::new_data(IMU_SEND_MSG_ID, &accel_bits).expect("Could not create frame");

        gyro_bits[0..2].copy_from_slice(&(((gyro.0 * 1000.0) as i16).to_be_bytes()));
        gyro_bits[2..4].copy_from_slice(&(((gyro.1 * 1000.0) as i16).to_be_bytes()));
        gyro_bits[4..].copy_from_slice(&(((gyro.2 * 1000.0) as i16).to_be_bytes()));

        let gyro_frame =
            Frame::new_data(GYRO_SEND_MSG_ID, &gyro_bits).expect("Could not create frame");

        can_send.send(accel_frame).await;
        can_send.send(gyro_frame).await;
    }
}

const TOF_REFRESH_TIME: Duration = Duration::from_millis(500);
const TOF_SEND_MSG_ID: StandardId = StandardId::new(0x607).expect("Could not parse ID");

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
            .send(unwrap!(Frame::new_data(TOF_SEND_MSG_ID, &range_bits)))
            .await;

        Timer::after(TOF_REFRESH_TIME).await;
    }
}

const ADC_REFRESH_TIME: Duration = Duration::from_millis(250);
const STRAIN_SEND_MSG_ID: StandardId = StandardId::new(0x606).expect("Could not parse ID");
const SHOCKPOT_SEND_MSG_ID: StandardId = StandardId::new(0x605).expect("Could not parse ID");

#[embassy_executor::task]
pub async fn adc1_reader(
    mut adc1: RingBufferedAdc<'static, ADC1>,
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
) {
    let mut measurements: [u16; 60] = [0u16; 120 / 2];
    let mut strain_bits: [u8; 4] = [0; 4];

    loop {
        match adc1.read(&mut measurements).await {
            Ok(_) => {
                adc1.teardown_adc();
                // TODO transform measurements
                strain_bits[0..2].copy_from_slice(&measurements[1].to_be_bytes());
                strain_bits[2..4].copy_from_slice(&measurements[2].to_be_bytes());
                can_send
                    .send(unwrap!(Frame::new_data(
                        STRAIN_SEND_MSG_ID,
                        &measurements[0].to_be_bytes()
                    )))
                    .await;
                can_send
                    .send(unwrap!(Frame::new_data(SHOCKPOT_SEND_MSG_ID, &strain_bits)))
                    .await;
            }
            Err(_) => {
                warn!("DMA overrun");
                continue;
            }
        }
        Timer::after(ADC_REFRESH_TIME).await;
    }
}
