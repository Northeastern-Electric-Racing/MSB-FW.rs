//! Generic CAN handler for NER STM32H5 firmware projects.
//!
//! This crate wraps Embassy's `embassy-stm32` FDCAN peripheral to provide a
//! ready-to-use Classical CAN configuration and an [`embassy_executor`] task
//! ([`can_handler`]) that bridges the CAN bus with the rest of a user program
//! over [`embassy_sync`] channels.
//!
//! The bus is configured for Classical CAN at 500 kbit/s. See [`NerCan::init`]
//! for the exact timing and filter configuration.
#![no_std]

use core::num::{NonZeroU8, NonZeroU16};

use defmt::{warn};
use embassy_futures::select::{Either, select};
use embassy_stm32::can::{CanConfigurator, Frame};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Receiver, Sender};

pub struct NerCan {
    can_configurator: CanConfigurator<'static>,
}

impl NerCan {
    /// This is the CAN configuration to be used by most NER Projects.
    /// This is for optional use to pass into the can_handler task to facilitate initialize
    ///
    /// The configuration sets:
    /// - Automatic bus-off recovery enabled.
    /// - Automatic retransmission disabled.
    /// - Classical CAN framing only (no CAN FD).
    /// - A clock divider of 1 and the data bit timing required for 500 kbit/s.
    /// - Transmit pause enabled.
    /// - A global filter that rejects all frames by default.
    ///
    /// The bitrate is set to 500 kbit/s.
    /// 
    /// ** It is expected that the user manually configures the CAn Std and Extended Filters before running the can_handler task
    pub fn init(mut self) {
        use embassy_stm32::can::config::*;
        let can_config = FdCanConfig::default()
            .set_automatic_bus_off_recovery(true)   
            .set_automatic_retransmit(false)
            .set_frame_transmit(FrameTransmissionConfig::ClassicCanOnly)
            .set_clock_divider(ClockDivider::_1)
            .set_data_bit_timing(DataBitTiming {
                transceiver_delay_compensation: false,
                prescaler: NonZeroU16::new(8).unwrap(),
                seg1: NonZeroU8::new(8).unwrap(),
                seg2: NonZeroU8::new(4).unwrap(),
                sync_jump_width: NonZeroU8::new(1).unwrap(),
            })
            .set_transmit_pause(true)
            .set_global_filter(GlobalFilter::reject_all());
        self.can_configurator.set_config(can_config);
        self.can_configurator.set_bitrate(500_000);
    }       
}

/// CAN handler Embassy task for generic use in STM32H5 projects.
///
/// Puts the configurator into normal mode and then services the bus in a loop,
///
/// **The `sender` and `receiver` are not intended to derive from the same
///
/// - `sender` passes on CAN frames received from the bus so they can be parsed
///   by the user program.
/// - `receiver` dispatches CAN frames queued by other threads in the user
///   program for transmission onto the bus.
///
#[embassy_executor::task]
pub async fn can_handler(can_configurator: CanConfigurator<'static>, sender: Sender<'static, ThreadModeRawMutex, Frame, 16>, receiver: Receiver<'static, ThreadModeRawMutex, Frame, 16>) {
    // Starts Classical CAN transmission and receival
    let mut can = can_configurator.into_normal_mode();

    // Loop to handle both receiving and sending
    loop {
        match select(receiver.receive(), can.read()).await {
        // Handle sending out a CAN message
        Either::First(frame) => {
                if can.write(&frame).await.is_some() {
                    warn!("Dequeing can frames!");
                }
            }
        // Handle receiving and CAN message and passing to another task
        Either::Second(res) => match res {      
                Ok(can_recv) => {
                    let frame = can_recv.frame;
                    let _ = sender.send(frame);
                }
                Err(err) => warn!("Bus error! {}", err),
            },
        }
    }
}
