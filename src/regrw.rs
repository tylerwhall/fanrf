use std::io::{self, Write};
use std::ops::DerefMut;

use spidev::{Spidev, SpidevTransfer};

pub trait RegRw {
    fn read(&mut self, reg: u8) -> io::Result<u8>;
    fn write(&mut self, reg: u8, val: u8) -> io::Result<()>;
    fn burst_write(&mut self, reg: u8, val: &[u8]) -> io::Result<()>;
}

// Not sure why this is required
impl<T: RegRw + ?Sized> RegRw for Box<T> {
    fn read(&mut self, reg: u8) -> io::Result<u8> {
        self.deref_mut().read(reg)
    }
    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        self.deref_mut().write(reg, val)
    }
    fn burst_write(&mut self, reg: u8, val: &[u8]) -> io::Result<()> {
        self.deref_mut().burst_write(reg, val)
    }
}

pub struct RfmRegs {
    spi: Spidev,
}

impl RfmRegs {
    pub fn new(spi: Spidev) -> Self {
        RfmRegs { spi: spi }
    }
}

impl RegRw for RfmRegs {
    fn read(&mut self, reg: u8) -> io::Result<u8> {
        let mut rbuf = [0u8, 0u8];
        let tbuf = [reg, 0u8];
        self.spi.transfer(&mut SpidevTransfer::read_write(&tbuf, &mut rbuf)).map(|_| rbuf[1])
    }

    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        self.spi.write_all(&[reg | 0x80, val])
    }

    fn burst_write(&mut self, reg: u8, val: &[u8]) -> io::Result<()> {
        let addr = [reg | 0x80];
        let mut tx = [SpidevTransfer::write(&addr), SpidevTransfer::write(val)];
        self.spi.transfer_multiple(&mut tx)
    }
}

pub struct FakeRegs([u8; 0x80]);

impl FakeRegs {
    pub fn new() -> Self {
        FakeRegs([0; 128])
    }
}

impl RegRw for FakeRegs {
    fn read(&mut self, reg: u8) -> io::Result<u8> {
        Ok(self.0[reg as usize])
    }

    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        self.0[reg as usize] = val;
        Ok(())
    }

    fn burst_write(&mut self, mut reg: u8, val: &[u8]) -> io::Result<()> {
        for byte in val {
            self.0[reg as usize] = *byte;
            if reg < 0x7f {
                // Auto-increment unless this is the fifo register
                reg += 1;
            }
        }
        Ok(())
    }
}

pub struct RegLogger<R: RegRw>(pub R);

impl<R: RegRw> RegRw for RegLogger<R> {
    fn read(&mut self, reg: u8) -> io::Result<u8> {
        self.0.read(reg).map(|val| {
            println!("Reg read  0x{:02x} = 0x{:02x}", reg, val);
            val
        })
    }

    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        println!("Reg write 0x{:02x} = 0x{:02x}", reg, val);
        self.0.write(reg, val)
    }

    fn burst_write(&mut self, reg: u8, val: &[u8]) -> io::Result<()> {
        println!("Burst({:2}) 0x{:02x} = {:?}", val.len(), reg, val);
        self.0.burst_write(reg, val)
    }
}

pub trait RfmReg {
    /// Get the register number for this reg struct
    fn regval() -> u8;
}
