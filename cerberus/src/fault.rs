use defmt::{debug, unwrap, warn};
use embassy_futures::select::select;
use embassy_stm32::can::{Frame, StandardId};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Sender,
    signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};

use crate::{FaultCode, FaultSeverity, FunctionalType, StateTransition};

#[embassy_executor::task]
pub async fn fault_handler(
    can_send: Sender<'static, ThreadModeRawMutex, Frame, 25>,
    fault: &'static Signal<CriticalSectionRawMutex, FaultCode>,
    state_send: &'static Signal<CriticalSectionRawMutex, StateTransition>,
) {
    let mut last_fault = FaultCode::FaultsClear;

    let status_id: StandardId = unwrap!(StandardId::new(0x502));

    let mut fault_bits: [u8; 5] = [0u8; 5];

    let mut last_fault_time = Instant::now();

    loop {
        last_fault = match select(fault.wait(), Timer::after_millis(250)).await {
            embassy_futures::select::Either::First(event) => {
                match event.get_severity() {
                    crate::FaultSeverity::Defcon1
                    | crate::FaultSeverity::Defcon2
                    | crate::FaultSeverity::Defcon3 => {
                        state_send.signal(StateTransition::Functional(FunctionalType::FAULTED));
                        last_fault_time = Instant::now();
                    }
                    crate::FaultSeverity::Defcon4 => warn!("Non critical fault!"),
                    crate::FaultSeverity::Defcon5 => debug!("Faults clear!"),
                }
                event
            }
            embassy_futures::select::Either::Second(_) => {
                if last_fault.get_severity() as u8 <= FaultSeverity::Defcon3 as u8
                    && Instant::now() - last_fault_time > Duration::from_secs(5)
                {
                    state_send.signal(StateTransition::Functional(FunctionalType::READY))
                }
                FaultCode::FaultsClear
            }
        };

        fault_bits[3..4].copy_from_slice(&(last_fault.get_severity() as u8).to_be_bytes());
        fault_bits[0..3].copy_from_slice(&(last_fault as u32).to_be_bytes());

        can_send
            .send(unwrap!(Frame::new_data(status_id, &fault_bits)))
            .await;
    }
}
