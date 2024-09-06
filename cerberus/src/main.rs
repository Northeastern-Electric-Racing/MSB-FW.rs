#![no_std]
#![no_main]

use core::fmt::Write;

use cerberus::{bms, can_handler, fault, FaultCode, SharedI2c, StateTransition};
use cortex_m::{peripheral::SCB, singleton};
use cortex_m_rt::{exception, ExceptionFrame};
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_stm32::{
    adc::{Adc, SampleTime, Sequence},
    bind_interrupts,
    can::{Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler},
    i2c::{self, I2c},
    peripherals::CAN1,
    time::Hertz,
};
use embassy_stm32::{
    can::Frame,
    gpio::{Level, Output, Speed},
    peripherals,
    usart::{self, Uart},
    wdg::IndependentWatchdog,
    Config,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, ThreadModeRawMutex},
    channel::Channel,
    mutex::Mutex,
    signal::Signal,
};
use embassy_time::Timer;
use heapless::String;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct IrqsCAN {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

bind_interrupts!(struct IrqsUsart {
    USART3 => usart::InterruptHandler<peripherals::USART3>;
});

bind_interrupts!(struct IrqsI2c1 {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

bind_interrupts!(struct IrqsI2c2 {
    I2C2_EV => i2c::EventInterruptHandler<peripherals::I2C2>;
    I2C2_ER => i2c::ErrorInterruptHandler<peripherals::I2C2>;
});

static CAN_CHANNEL: Channel<ThreadModeRawMutex, Frame, 25> = Channel::new();

static CURRENT_STATE: Signal<CriticalSectionRawMutex, StateTransition> = Signal::new();
static FAULT: Signal<CriticalSectionRawMutex, FaultCode> = Signal::new();

static BMS_CALLBACK: Signal<CriticalSectionRawMutex, Frame> = Signal::new();
static DTI_CALLBACK: Signal<CriticalSectionRawMutex, Frame> = Signal::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    info!("Initializing Cerberus...");

    let mut p = embassy_stm32::init(Config::default());

    let can = Can::new(p.CAN1, p.PA11, p.PA12, IrqsCAN);
    if let Err(err) = spawner.spawn(can_handler::can_handler(
        can,
        &BMS_CALLBACK,
        &DTI_CALLBACK,
        CAN_CHANNEL.receiver(),
    )) {
        warn!("Could not spawn CAN task: {}", err);
    }

    if let Err(err) = spawner.spawn(bms::bms_handler(&BMS_CALLBACK, &FAULT)) {
        warn!("Could not spawn BMS task: {}", err);
    }

    if let Err(err) = spawner.spawn(fault::fault_handler(CAN_CHANNEL.sender(), &FAULT)) {
        warn!("Could not spawn fault task: {}", err);
    }

    static I2C_BUS_1: StaticCell<SharedI2c> = StaticCell::new();
    let i2c_1 = I2c::new(
        p.I2C1,
        p.PB6,
        p.PB7,
        IrqsI2c1,
        p.DMA1_CH6,
        p.DMA1_CH0,
        Hertz(100_000),
        i2c::Config::default(),
    );
    let i2c_bus_1 = I2C_BUS_1.init(Mutex::new(i2c_1));

    static I2C_BUS_2: StaticCell<SharedI2c> = StaticCell::new();
    let i2c_2 = I2c::new(
        p.I2C2,
        p.PB10,
        p.PB11,
        IrqsI2c2,
        p.DMA1_CH7,
        p.DMA1_CH2,
        Hertz(100_000),
        i2c::Config::default(),
    );
    let i2c_bus_2 = I2C_BUS_2.init(Mutex::new(i2c_2));

    const ADC_BUF_SIZE: usize = 1024;

    let adc1 = Adc::new(p.ADC1);
    let adc_data_1 = singleton!(ADCDAT : [u16; ADC_BUF_SIZE] = [0u16; ADC_BUF_SIZE])
        .expect("Could not init adc buffer");
    let mut adc1 = adc1.into_ring_buffered(p.DMA2_CH4, adc_data_1);
    adc1.set_sample_sequence(Sequence::One, &mut p.PB0, SampleTime::CYCLES112); //

    let adc3 = Adc::new(p.ADC3);
    let adc_data_3 = singleton!(ADCDAT : [u16; ADC_BUF_SIZE] = [0u16; ADC_BUF_SIZE])
        .expect("Could not init adc buffer");
    let mut adc3 = adc3.into_ring_buffered(p.DMA2_CH0, adc_data_3);
    adc3.set_sample_sequence(Sequence::One, &mut p.PA0, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA1, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA2, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA3, SampleTime::CYCLES112); //

    let mut usart = Uart::new(
        p.USART3,
        p.PC11,
        p.PC10,
        IrqsUsart,
        p.DMA1_CH3,
        p.DMA1_CH1,
        usart::Config::default(),
    )
    .unwrap();
    let mut s: String<128> = String::new();
    core::write!(&mut s, "Hello DMA World!\r\n",).unwrap();
    unwrap!(usart.write(s.as_bytes()).await);

    let mut watchdog = IndependentWatchdog::new(p.IWDG, 4000000);
    watchdog.unleash();
    let mut led_pin = Output::new(p.PC8, Level::Low, Speed::Low);
    loop {
        info!("Status: Alive");
        led_pin.toggle();
        Timer::after_secs(3).await;
        watchdog.pet();
    }
}

#[exception]
unsafe fn HardFault(_frame: &ExceptionFrame) -> ! {
    SCB::sys_reset() // <- you could do something other than reset
}
