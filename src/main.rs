extern crate spidev;

use spidev::{Spidev, SpidevOptions, SpidevTransfer};

fn main() {
    let mut spi = Spidev::open("/dev/spidev1.0").unwrap();
    let options = SpidevOptions::new()
        .max_speed_hz(10 * 1000 * 1000)
        .build();
    spi.configure(&options).unwrap();
    let mut rbuf = [0u8, 0u8];
    let tbuf  = [0x12u8, 0u8];
    {
        let mut t = SpidevTransfer::read_write(&tbuf, &mut rbuf);
        spi.transfer(&mut t).unwrap();
    }
    println!("Hello, world! {:#x}", rbuf[1]);
}
