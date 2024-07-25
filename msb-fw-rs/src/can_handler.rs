use defmt::{trace, unwrap};
use embassy_stm32::{
    can::{bxcan::Frame, Can},
    peripherals::CAN1,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Receiver};
use embassy_time::Timer;

use crate::DeviceLocation;

#[embassy_executor::task]
pub async fn can_handler(
    mut can: Can<'static, CAN1>,
    recv: Receiver<'static, ThreadModeRawMutex, Frame, 25>,
    loc: DeviceLocation,
) {
    can.set_bitrate(1_000_000);
    can.enable().await;

    loop {
        let frame = recv.receive().await;
        let frame_fixed = Frame::new_data(loc.get_can_id(frame.id()), *unwrap!(frame.data()));
        trace!("Sending frame: {}", frame_fixed);
        can.write(&frame_fixed).await;

        Timer::after_millis(5).await;
    }
}
