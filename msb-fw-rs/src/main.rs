#![no_std]
#![no_main]

use core::fmt::Write;
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_stm32::{
    can::bxcan::Frame,
    peripherals,
    usart::{self, Uart},
    wdg::IndependentWatchdog,
    Config,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::Timer;
use heapless::String;
use msb_fw_rs::can_handler;
use {
    defmt_rtt as _,
    embassy_stm32::{
        bind_interrupts,
        can::{
            Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler,
        },
        peripherals::CAN1,
    },
    panic_probe as _,
};

bind_interrupts!(struct IrqsCAN {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

bind_interrupts!(struct IrqsUsart {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
});

static CAN_CHANNEL: Channel<ThreadModeRawMutex, Frame, 25> = Channel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_stm32::init(Config::default());

    let can = Can::new(p.CAN1, p.PA11, p.PA12, IrqsCAN);
    if let Err(err) = spawner.spawn(can_handler::can_handler(can, CAN_CHANNEL.receiver())) {
        warn!("Could not spawn CAN task: {}", err);
    }

    let mut usart = Uart::new(
        p.USART2,
        p.PA3,
        p.PA2,
        IrqsUsart,
        p.DMA1_CH6,
        p.DMA1_CH5,
        usart::Config::default(),
    )
    .unwrap();
    let mut s: String<128> = String::new();
    core::write!(&mut s, "Hello DMA World!\r\n",).unwrap();
    unwrap!(usart.write(s.as_bytes()).await);

    let mut watchdog = IndependentWatchdog::new(p.IWDG, 4000000);
    watchdog.unleash();
    loop {
        info!("Status: Alive");
        Timer::after_secs(3).await;
        watchdog.pet();
    }
}
