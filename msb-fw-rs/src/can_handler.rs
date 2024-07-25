use defmt::trace;
use embassy_stm32::{
    can::{bxcan::Frame, Can},
    peripherals::CAN1,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Receiver};
use embassy_time::Timer;

#[embassy_executor::task]
pub async fn can_handler(
    mut can: Can<'static, CAN1>,
    recv: Receiver<'static, ThreadModeRawMutex, Frame, 25>,
) {
    can.set_bitrate(1_000_000);
    can.enable().await;

    loop {
        let frame = recv.receive().await;
        trace!("Sending frame: {}", frame);
        can.write(&frame).await;

        Timer::after_millis(5).await;
    }
}
