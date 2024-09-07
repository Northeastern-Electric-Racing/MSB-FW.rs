#![no_std]
#![no_main]

use core::{
    fmt::Write,
    sync::atomic::{AtomicBool, AtomicI32},
};

use cerberus::{
    bms, can_handler, dti, fault, monitor, state_machine, FaultCode, PduCommand, SharedI2c,
    StateTransition,
};
use cortex_m::{peripheral::SCB, singleton};
use cortex_m_rt::{exception, ExceptionFrame};
use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_stm32::{
    adc::{Adc, SampleTime, Sequence},
    bind_interrupts,
    can::{Can, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler},
    exti::ExtiInput,
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

// channels to pass info with backpressure
static CAN_CHANNEL: Channel<ThreadModeRawMutex, Frame, 25> = Channel::new();
static PDU_COMMAND: Channel<ThreadModeRawMutex, PduCommand, 10> = Channel::new();

// signals for most up to date state only

static CURRENT_STATE: Signal<CriticalSectionRawMutex, StateTransition> = Signal::new();
static FAULT: Signal<CriticalSectionRawMutex, FaultCode> = Signal::new();

// callbacks for CAN messages
static BMS_CALLBACK: Signal<CriticalSectionRawMutex, Frame> = Signal::new();
static DTI_CALLBACK: Signal<CriticalSectionRawMutex, Frame> = Signal::new();

// state that is checked periodically rather than awaited

// true=TS ON
static TSMS_SENSE: AtomicBool = AtomicBool::new(false);
// true=brakes engaged
static BRAKE_STATE: AtomicBool = AtomicBool::new(false);

static DTI_MPH: AtomicI32 = AtomicI32::new(0);

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
    if let Err(err) = spawner.spawn(dti::dti_handler(&DTI_CALLBACK, &DTI_MPH)) {
        warn!("Could not spawn DTI task: {}", err);
    }

    if let Err(err) = spawner.spawn(fault::fault_handler(
        CAN_CHANNEL.sender(),
        &FAULT,
        &CURRENT_STATE,
    )) {
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
    if let Err(err) = spawner.spawn(monitor::ctrl_expander_handler(
        CAN_CHANNEL.sender(),
        PDU_COMMAND.receiver(),
        i2c_bus_2,
        &TSMS_SENSE,
    )) {
        warn!("Could not spawn ctrl expander task: {}", err);
    }

    const ADC1_BUF_SIZE: usize = 40;
    let adc1 = Adc::new(p.ADC1);
    let adc_data_1 = singleton!(ADCDAT : [u16; ADC1_BUF_SIZE] = [0u16; ADC1_BUF_SIZE])
        .expect("Could not init adc buffer");
    let mut adc1 = adc1.into_ring_buffered(p.DMA2_CH4, adc_data_1);
    adc1.set_sample_sequence(Sequence::One, &mut p.PB0, SampleTime::CYCLES112); // LV sense
    if let Err(err) = spawner.spawn(monitor::lv_sense_handler(adc1, CAN_CHANNEL.sender())) {
        warn!("Could not spawn LV sense task: {}", err);
    }

    const ADC3_BUF_SIZE: usize = 120;
    let adc3 = Adc::new(p.ADC3);
    let adc_data_3 = singleton!(ADCDAT : [u16; ADC3_BUF_SIZE] = [0u16; ADC3_BUF_SIZE])
        .expect("Could not init adc buffer");
    let mut adc3 = adc3.into_ring_buffered(p.DMA2_CH0, adc_data_3);
    adc3.set_sample_sequence(Sequence::One, &mut p.PA0, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA1, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA2, SampleTime::CYCLES112); //
    adc3.set_sample_sequence(Sequence::One, &mut p.PA3, SampleTime::CYCLES112); //

    let button1 = ExtiInput::new(p.PA4, p.EXTI4, embassy_stm32::gpio::Pull::Up);
    let button2 = ExtiInput::new(p.PA5, p.EXTI5, embassy_stm32::gpio::Pull::Up);
    let button3 = ExtiInput::new(p.PA6, p.EXTI6, embassy_stm32::gpio::Pull::Up);
    let button4 = ExtiInput::new(p.PA7, p.EXTI7, embassy_stm32::gpio::Pull::Up);
    //let button5 = ExtiInput::new(p.PC4, p.EXTI4, embassy_stm32::gpio::Pull::Up);
    //let button6 = ExtiInput::new(p.PC5, p.EXTI5, embassy_stm32::gpio::Pull::Up);
    let button7 = ExtiInput::new(p.PB0, p.EXTI0, embassy_stm32::gpio::Pull::Up);
    let button8 = ExtiInput::new(p.PB1, p.EXTI1, embassy_stm32::gpio::Pull::Up);
    if let Err(err) = spawner.spawn(monitor::steeringio_handler(
        CAN_CHANNEL.sender(),
        button1,
        button2,
        button3,
        button4,
        // button5,
        // button6,
        button7,
        button8,
    )) {
        warn!("Could not spawn steeringIO task: {}", err);
    }

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

    if let Err(err) = spawner.spawn(state_machine::state_handler(
        &CURRENT_STATE,
        PDU_COMMAND.sender(),
        &DTI_MPH,
        &BRAKE_STATE,
        &TSMS_SENSE,
    )) {
        warn!("Could not spawn BMS task: {}", err);
    }

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
