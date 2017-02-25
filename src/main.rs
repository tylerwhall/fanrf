#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate clap;
extern crate spidev;
#[macro_use]
extern crate log;
extern crate env_logger;

mod regrw;
mod rfm;

use std::env;

use clap::{Arg, ArgMatches, App};
use env_logger::LogBuilder;
use log::LogLevelFilter;
use spidev::{Spidev, SpidevOptions};

use rfm::*;

#[repr(u8)]
#[derive(Copy, Clone)]
enum FanCmd12 {
    Light = 1,
}

#[derive(Clone, Debug, PartialEq)]
struct FanPkt12 {
    addr: u8,
    cmd: u8,
}

impl FanPkt12 {
    fn new(addr: u8, cmd: FanCmd12) -> Self {
        FanPkt12 {
            addr: addr,
            cmd: cmd as u8,
        }
    }
}

impl<'a> IntoIterator for &'a FanPkt12 {
    type Item = bool;
    type IntoIter = FanPkt12Bits<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FanPkt12Bits::new(self)
    }
}

#[derive(Clone)]
struct FanPkt12Bits<'a> {
    pkt: &'a FanPkt12,
    count: u8,
}

impl<'a> FanPkt12Bits<'a> {
    fn new(pkt: &'a FanPkt12) -> Self {
        FanPkt12Bits {
            pkt: pkt,
            count: 0,
        }
    }
}

impl<'a> Iterator for FanPkt12Bits<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.count {
            0 => Some(false), // Start bit
            1 => Some(true), // First bit is a 1
            2...5 => Some((self.pkt.addr & (1 << (3 - (self.count - 2))) != 0)),
            6...12 => Some((self.pkt.cmd as u8 & (1 << (6 - (self.count - 6))) != 0)),
            _ => return None,
        };
        self.count += 1;
        ret
    }
}

#[test]
fn fan12_serializer() {
    fn from_iter<I: Iterator<Item = bool>>(mut iter: I) -> FanPkt12 {
        assert_eq!(iter.next().unwrap(), false); // Start bit
        assert_eq!(iter.next().unwrap(), true); // First 1 bit
        let addr = if iter.next().unwrap() { 1 << 3 } else { 0 } |
                   if iter.next().unwrap() { 1 << 2 } else { 0 } |
                   if iter.next().unwrap() { 1 << 1 } else { 0 } |
                   if iter.next().unwrap() { 1 << 0 } else { 0 };
        let cmd = if iter.next().unwrap() { 1 << 6 } else { 0 } |
                  if iter.next().unwrap() { 1 << 5 } else { 0 } |
                  if iter.next().unwrap() { 1 << 4 } else { 0 } |
                  if iter.next().unwrap() { 1 << 3 } else { 0 } |
                  if iter.next().unwrap() { 1 << 2 } else { 0 } |
                  if iter.next().unwrap() { 1 << 1 } else { 0 } |
                  if iter.next().unwrap() { 1 << 0 } else { 0 };
        assert!(iter.next().is_none());
        FanPkt12 {
            addr: addr,
            cmd: cmd,
        }
    }
    for addr in 0..16 {
        for cmd in 0..128 {
            let pkt = FanPkt12 {
                addr: addr,
                cmd: cmd,
            };
            assert_eq!(pkt.clone(), from_iter(pkt.into_iter()));
        }
    }
}

#[derive(Clone)]
enum FanExpandState {
    Start,
    Data,
    End,
}

/// Adapts a data bit stream to 3 symbols per bit
#[derive(Clone)]
struct FanExpand<I: Iterator<Item = bool>>(I, FanExpandState);

impl<I: Iterator<Item = bool>> FanExpand<I> {
    fn new(iter: I) -> Self {
        FanExpand(iter, FanExpandState::Start)
    }
}

impl<I: Iterator<Item = bool>> Iterator for FanExpand<I> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        match self.1 {
            FanExpandState::Start => {
                let val = self.0.next();
                if val.is_some() {
                    self.1 = FanExpandState::Data;
                }
                val
            }
            FanExpandState::Data => {
                self.1 = FanExpandState::End;
                Some(true)
            }
            FanExpandState::End => {
                self.1 = FanExpandState::Start;
                Some(false)
            }
        }
    }
}

macro_rules! SPIDEV_DEFAULT { () => ("/dev/spidev1.0") }
macro_rules! TX_POWER_DEFAULT { () => (3) }

fn arg_parse<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .arg(Arg::with_name("spidev")
            .short("s")
            .long("spidev")
            .help(concat!("Linux spidev device. Deafults to ", SPIDEV_DEFAULT!()))
            .takes_value(true))
        .arg(Arg::with_name("irq")
            .short("i")
            .long("irq")
            .help("IRQ gpio number")
            .takes_value(true))
        .arg(Arg::with_name("shutdown")
            .short("n")
            .long("shutdown")
            .help("Shutdown gpio number")
            .takes_value(true))
        .arg(Arg::with_name("txpower")
            .short("p")
            .long("txpower")
            .help(concat!("Transmit power. Range 0-7. Defaults to ", stringify!(TX_POWER_DEFAULT!())))
            .takes_value(true))
        .arg(Arg::with_name("verbose")
            .short("v")
            .long("verbose")
            .help("Verbose logging"))
        .arg(Arg::with_name("debug")
            .short("d")
            .long("debug")
            .help("Debug logging (implies debug)"))
        .get_matches()
}

fn log_init(matches: &ArgMatches) {
    let mut log_builder = LogBuilder::new();
    if let Ok(log) = env::var("RUST_LOG") {
        log_builder.parse(&log);
    } else if matches.is_present("debug") {
        log_builder.filter(None, LogLevelFilter::Debug);
    } else if matches.is_present("verbose") {
        log_builder.filter(None, LogLevelFilter::Info);
    } else {
        log_builder.filter(None, LogLevelFilter::Warn);
    }
    log_builder.init().unwrap();
}

fn main() {
    let matches = arg_parse();
    log_init(&matches);
    let txpower = matches.value_of("txpower")
        .map(|p| p.parse::<u8>().expect("Invalid argument for txpower"))
        .unwrap_or(TX_POWER_DEFAULT!());
    if txpower > 7 {
        panic!("Requested TX power out of range.");
    }

    let spidev_path = matches.value_of("spidev").unwrap_or(SPIDEV_DEFAULT!());
    let mut rf = if let Ok(mut spi) = Spidev::open(spidev_path) {
        let options = SpidevOptions::new()
            .max_speed_hz(10 * 1000 * 1000)
            .build();
        spi.configure(&options).unwrap();
        Rfm22::new(spi)
    } else {
        warn!("Using dummy backend.");
        // Set FIFO to almost empty to we don't get stuck waiting on it
        Rfm22::dummy()
    };

    rf.init();
    rf.set_modulation_type_and_source(ModulationType::OOK, DataSource::FIFO).unwrap();
    rf.regs.write_validate(DataAccessControl::empty()).unwrap();
    // HeaderControl2
    rf.regs.write_validate(SKIPSYN).unwrap();
    rf.set_freq_mhz(303.8).unwrap();
    rf.set_data_rate_hz(3000.0).unwrap();
    rf.set_tx_power(txpower).unwrap();

    let pkt = FanPkt12::new(0x9, FanCmd12::Light);
    let bits = std::iter::repeat(FanExpand::new(pkt.into_iter())
                                 .chain(std::iter::repeat(false).take(11 * 3))) // 11ms pause between commands. 1/3ms symbol period
        .cycle()
        .take(20)
        .flat_map(|i| i);

    rf.transmit_bitstream(bits).unwrap();
}
