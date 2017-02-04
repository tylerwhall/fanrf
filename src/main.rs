extern crate spidev;

use spidev::{Spidev, SpidevOptions, SpidevTransfer};
use std::io;

struct Rfm22 {
    spi: Spidev,
}

impl Rfm22 {
    fn new(spi: Spidev) -> Self {
        Rfm22 { spi: spi }
    }

    fn read_reg(&mut self, reg: u8) -> io::Result<u8> {
        let mut rbuf = [0u8, 0u8];
        let tbuf = [reg, 0u8];
        self.spi.transfer(&mut SpidevTransfer::read_write(&tbuf, &mut rbuf)).map(|_| rbuf[1])
    }
}

fn main() {
    let mut spi = Spidev::open("/dev/spidev1.0").unwrap();
    let options = SpidevOptions::new()
        .max_speed_hz(10 * 1000 * 1000)
        .build();
    spi.configure(&options).unwrap();

    let mut rf = Rfm22::new(spi);
    println!("Hello, world! {:#x}", rf.read_reg(0x7).unwrap());
}
