extern crate spidev;

use spidev::{Spidev, SpidevOptions, SpidevTransfer};
use std::io::{self, Write};

struct RfmRegs {
    spi: Spidev,
}

impl RfmRegs {
    fn new(spi: Spidev) -> Self {
        RfmRegs { spi: spi }
    }

    fn read(&mut self, reg: u8) -> io::Result<u8> {
        let mut rbuf = [0u8, 0u8];
        let tbuf = [reg, 0u8];
        self.spi.transfer(&mut SpidevTransfer::read_write(&tbuf, &mut rbuf)).map(|_| rbuf[1])
    }

    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        self.spi.write_all(&[reg | 0x80, val])
    }
}

#[repr(u8)]
enum Rfm22Regs {
    OperatingFunctionControl1 = 0x7,
}

struct Rfm22 {
    regs: RfmRegs,
}

impl Rfm22 {
    fn new(spi: Spidev) -> Self {
        Rfm22 { regs: RfmRegs::new(spi) }
    }

    fn read(&mut self, reg: Rfm22Regs) -> io::Result<u8> {
        self.regs.read(reg as u8)
    }

    fn write(&mut self, reg: Rfm22Regs, val: u8) -> io::Result<()> {
        self.regs.write(reg as u8, val)
    }

    pub fn init(&mut self) {
        self.write(Rfm22Regs::OperatingFunctionControl1, 0x3).unwrap();
        assert_eq!(self.read(Rfm22Regs::OperatingFunctionControl1).unwrap(), 0x3);
    }
}

fn main() {
    let mut spi = Spidev::open("/dev/spidev1.0").unwrap();
    let options = SpidevOptions::new()
        .max_speed_hz(10 * 1000 * 1000)
        .build();
    spi.configure(&options).unwrap();

    let mut rf = Rfm22::new(spi);
    rf.init();
}
