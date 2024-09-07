use core::{f32::consts::PI, sync::atomic::AtomicI32};

use embassy_stm32::can::Frame;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

#[embassy_executor::task]
pub async fn dti_handler(
    recved: &'static Signal<CriticalSectionRawMutex, Frame>,
    speed: &'static AtomicI32,
) {
    loop {
        let frame = recved.wait().await;
        match frame.id() {
            embassy_stm32::can::Id::Standard(id) => match id.as_raw() {
                0x416 => {
                    // TODO fat chance this works
                    let erpm = ((frame.data()[0] as i32) << 24u32)
                        + ((frame.data()[1] as i32) << 16)
                        + ((frame.data()[2] as i32) << 8u32)
                        + (frame.data()[3] as i32);
                    let mph = (erpm / 10) as f32 / (47.0 / 13.0) * 60.0 * (16.0 / 63360.0) * PI;
                    // TODO add precision
                    speed.store(mph as i32, core::sync::atomic::Ordering::Release);
                }
                _ => (),
            },
            embassy_stm32::can::Id::Extended(_) => (),
        }
    }
}
