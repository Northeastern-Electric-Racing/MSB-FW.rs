use defmt::{trace, unwrap, warn};
use embassy_futures::select;
use embassy_futures::select::select;
use embassy_stm32::can::{
    filter::{BankConfig, ListEntry16},
    Can, Frame, StandardId,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Receiver,
    signal::Signal,
};

#[embassy_executor::task]
pub async fn can_handler(
    mut can: Can<'static>,
    bms_callback: &'static Signal<CriticalSectionRawMutex, Frame>,
    dti_callback: &'static Signal<CriticalSectionRawMutex, Frame>,
    recv: Receiver<'static, ThreadModeRawMutex, Frame, 25>,
) {
    can.set_bitrate(500_000);
    can.modify_filters().enable_bank(
        0,
        embassy_stm32::can::Fifo::Fifo0,
        BankConfig::List16([
            ListEntry16::data_frames_with_id(unwrap!(StandardId::new(0x156))),
            ListEntry16::data_frames_with_id(unwrap!(StandardId::new(0x416))),
            ListEntry16::data_frames_with_id(unwrap!(StandardId::new(0x1))), // TODO needed?
            ListEntry16::data_frames_with_id(unwrap!(StandardId::new(0x2))),
        ]),
    );
    can.enable().await;

    loop {
        match select(recv.receive(), can.read()).await {
            select::Either::First(frame) => {
                trace!("Sending frame: {}", frame);
                if let Some(_) = can.write(&frame).await.dequeued_frame() {
                    warn!("Dequeing can frames!");
                }
            }
            select::Either::Second(res) => match res {
                Ok(got) => match got.frame.header().id() {
                    embassy_stm32::can::Id::Standard(header) => match header.as_raw() {
                        0x416 => dti_callback.signal(got.frame),
                        0x156 => bms_callback.signal(got.frame),
                        _ => warn!("Ignored message of id {}", header.as_raw()),
                    },
                    embassy_stm32::can::Id::Extended(header) => {
                        warn!("Ignored message of ext. id {}", header.as_raw())
                    }
                },
                Err(err) => warn!("Bus error! {}", err),
            },
        }
    }
}
