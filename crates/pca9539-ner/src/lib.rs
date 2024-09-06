#![no_std]
use bit_field::BitField;
use embedded_hal_async::i2c::I2c;
use num_enum::TryFromPrimitive;

/// Defines errors
#[derive(Debug, Copy, Clone)]
pub enum Error<E> {
    /// Underlying bus error
    BusError(E),
}

impl<E> From<E> for Error<E> {
    fn from(error: E) -> Self {
        Error::BusError(error)
    }
}

/// Pin modes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    /// Represents input mode.
    Input = 1,
    /// Represents output mode.
    Output = 0,
}

/// Pin levels.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Level {
    /// High level
    High = 1,
    /// Low level
    Low = 0,
}

/// Pin names
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromPrimitive, Default)]
#[repr(u8)]
pub enum Pin {
    #[default]
    P00 = 0,
    P01 = 1,
    P02 = 2,
    P03 = 3,
    P04 = 4,
    P05 = 5,
    P06 = 6,
    P07 = 7,
    P10 = 8,
    P11 = 9,
    P12 = 10,
    P13 = 11,
    P14 = 12,
    P15 = 13,
    P16 = 14,
    P17 = 15,
}

/// PCA9539/TCA9539 is a 16-pin I2C I/O Expander with I2C Interface.
#[derive(Clone, Copy, Debug)]
pub struct Pca9539<I2C> {
    i2c: I2C,
    address: u8,
}

///
impl<I2C: I2c, E> Pca9539<I2C>
where
    I2C: I2c<Error = E>,
{
    const DEFAULT_ADDRESS: u8 = 0x74;

    /// Creates an expander with the default configuration.
    pub fn new_default(i2c: I2C) -> Result<Self, Error<E>> {
        Self::new(i2c, Self::DEFAULT_ADDRESS)
    }

    /// Creates an expander with specific address.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Error<E>> {
        Ok(Self { i2c, address })
    }

    /// Return the I2C address
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read an 8 bit register
    pub async fn read(&mut self, addr: u8) -> Result<u8, E> {
        let mut data = [0u8];
        self.i2c
            .write_read(self.address, &[addr], &mut data)
            .await?;
        Ok(data[0])
    }

    /// Write an 8 bit register
    pub async fn write(&mut self, addr: u8, data: u8) -> Result<(), E> {
        self.i2c.write(self.address, &[addr, data]).await
    }

    /// Get a single bit in a register
    pub async fn bit(&mut self, addr: u8, bit: usize) -> Result<bool, E> {
        let data = self.read(addr).await?;
        Ok(data.get_bit(bit))
    }

    /// Set a single bit in a register
    pub async fn set_bit(&mut self, addr: u8, bit: usize, value: bool) -> Result<(), E> {
        let mut data = self.read(addr).await?;
        data.set_bit(bit, value);
        self.write(addr, data).await
    }

    pub async fn set_level(&mut self, pin: Pin, level: Level) -> Result<(), E> {
        let pin = pin as u8;
        self.set_bit(0x02 | (pin >> 3), pin as usize & 7, level == Level::High)
            .await
    }

    pub async fn set_direction(&mut self, pin: Pin, direction: Direction) -> Result<(), E> {
        let pin = pin as u8;
        self.set_bit(
            0x06 | (pin >> 3),
            pin as usize & 7,
            direction == Direction::Input,
        )
        .await
    }
}
