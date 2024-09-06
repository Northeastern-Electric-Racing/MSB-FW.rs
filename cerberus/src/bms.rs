use embassy_futures::select::select;
use embassy_stm32::can::Frame;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

use crate::FaultCode;

#[embassy_executor::task]
pub async fn bms_handler(
    recved: &'static Signal<CriticalSectionRawMutex, Frame>,
    fault: &'static Signal<CriticalSectionRawMutex, FaultCode>,
) {
    loop {
        match select(recved.wait(), Timer::after_secs(4)).await {
            embassy_futures::select::Either::First(_) => continue,
            embassy_futures::select::Either::Second(_) => {
                fault.signal(FaultCode::BmsCanMonitorFault)
            }
        }
    }
}
