use defmt::{trace, unwrap, warn};
use embassy_stm32::can::{Can, Frame};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Receiver};

use crate::DeviceLocation;

#[embassy_executor::task]
pub async fn can_handler(
    mut can: Can<'static>,
    recv: Receiver<'static, ThreadModeRawMutex, Frame, 25>,
    loc: DeviceLocation,
) {
    can.set_bitrate(500_000);
    can.enable().await;

    loop {
        let frame = recv.receive().await;
        let frame_fixed = unwrap!(Frame::new_data(loc.get_can_id(frame.id()), frame.data()));
        trace!("Sending frame: {}", frame_fixed);
        if can.write(&frame_fixed).await.dequeued_frame().is_some() {
            warn!("Dequeing can frames!");
        }

        //Timer::after_millis(5).await;
    }
}
