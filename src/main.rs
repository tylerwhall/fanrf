extern crate spidev;

use spidev::{Spidev, SpidevTransfer};

fn main() {
    let spi = Spidev::open("/dev/spidev1.0").unwrap();
    let mut rbuf = [0u8];
    let tbuf  = [0x7u8];
    {
        let mut t = SpidevTransfer::read_write(&tbuf, &mut rbuf);
        spi.transfer(&mut t).unwrap();
    }
    println!("Hello, world! {:#x}", rbuf[0]);
}
