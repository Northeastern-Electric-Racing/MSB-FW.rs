#![no_std]
#![no_main]

use core::{cell::RefCell, fmt::Write};
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    can::{Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler},
    dma::NoDma,
    i2c::{self, I2c},
    peripherals::CAN1,
    time::Hertz,
};
use embassy_stm32::{
    can::bxcan::Frame,
    gpio::{Input, Level, Output, Pull, Speed},
    peripherals,
    usart::{self, Uart},
    wdg::IndependentWatchdog,
    Config,
};
use embassy_sync::{
    blocking_mutex::{raw::ThreadModeRawMutex, Mutex},
    channel::Channel,
};
use embassy_time::Timer;
use heapless::String;
use msb_fw_rs::{can_handler, controllers, readers, DeviceLocation, SharedI2c3};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct IrqsCAN {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

bind_interrupts!(struct IrqsUsart {
    USART2 => usart::InterruptHandler<peripherals::USART2>;
});

bind_interrupts!(struct IrqsI2c {
    I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
    I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
});

static CAN_CHANNEL: Channel<ThreadModeRawMutex, Frame, 25> = Channel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_stm32::init(Config::default());

    let pin0 = Input::new(p.PC10, Pull::None);
    let addr0 = pin0.get_level() == Level::High;

    let pin1 = Input::new(p.PC11, Pull::None);
    let addr1 = pin1.get_level() == Level::High;

    let pin2 = Input::new(p.PC12, Pull::None);
    let addr2 = pin2.get_level() == Level::High;

    let loc = DeviceLocation::from((addr0, addr1, addr2));

    let led1 = Output::new(p.PC4, Level::High, Speed::Low);
    let led2 = Output::new(p.PC5, Level::High, Speed::Low);
    if let Err(err) = spawner.spawn(controllers::control_leds(
        led1.degrade(),
        led2.degrade(),
        loc.clone(),
    )) {
        warn!("Could not spawn CAN task: {}", err);
    }

    let can = Can::new(p.CAN1, p.PA11, p.PA12, IrqsCAN);
    if let Err(err) = spawner.spawn(can_handler::can_handler(can, CAN_CHANNEL.receiver(), loc)) {
        warn!("Could not spawn CAN task: {}", err);
    }

    // checkout this fuckery, the official way to have two things use one i2c bus
    static I2C_BUS: StaticCell<SharedI2c3> = StaticCell::new();
    let i2c = I2c::new(
        p.I2C3,
        p.PA8,
        p.PC9,
        IrqsI2c,
        NoDma,
        NoDma,
        Hertz(100_000),
        i2c::Config::default(),
    );
    let i2c_bus = I2C_BUS.init(Mutex::new(RefCell::new(i2c)));
    if let Err(err) = spawner.spawn(readers::temperature_reader(i2c_bus, CAN_CHANNEL.sender())) {
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
