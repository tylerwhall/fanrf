#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate clap;
extern crate spidev;
extern crate sysfs_gpio;
#[macro_use]
extern crate log;
extern crate env_logger;

mod regrw;
mod rfm;

use std::env;
use std::iter::repeat;

use clap::{Arg, ArgMatches, App, AppSettings, SubCommand};
use env_logger::LogBuilder;
use log::LogLevelFilter;
use spidev::{Spidev, SpidevOptions};
use sysfs_gpio::Pin;

use rfm::*;

enum FanPkt {
    Dumb(FanPkt12),
    Smart(FanPkt21),
}

impl FanPkt {
    fn transmit(&self, rf: &mut Rfm22) {
        fn send_pkt<I: IntoIterator<Item = bool>>(rf: &mut Rfm22, iter: I, count: usize)
            where I::IntoIter: Clone
        {
            let bits = repeat(FanExpand::new(repeat(false).take(1) // Start bit
                                             .chain(iter.into_iter()))
                              .chain(std::iter::repeat(false).take(11 * 3))) // 11ms pause between commands. 1/3ms symbol period
                .cycle()
                .take(count)
                .flat_map(|i| i);
            rf.transmit_bitstream(bits).unwrap();
        }

        match *self {
            FanPkt::Dumb(ref pkt) => send_pkt(rf, pkt, 20),
            FanPkt::Smart(ref pkt) => send_pkt(rf, pkt, 30),
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum FanCmd12 {
    Light = 0x01,
    FanHigh = 0x20,
    FanMed = 0x10,
    FanLow = 0x08,
    FanOff = 0x02,
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
            0 => Some(true), // First bit is a 1
            1...4 => Some((self.pkt.addr & (1 << (3 - (self.count - 1))) != 0)),
            5...11 => Some((self.pkt.cmd as u8 & (1 << (6 - (self.count - 5))) != 0)),
            _ => return None,
        };
        self.count += 1;
        ret
    }
}

#[test]
fn fan12_serializer() {
    fn from_iter<I: Iterator<Item = bool>>(mut iter: I) -> FanPkt12 {
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

fn reverse_nibble(n: u8) -> u8 {
    (n & (1 << 3)) >> 3 | (n & (1 << 2)) >> 1 | (n & (1 << 1)) << 1 | (n & (1 << 0)) << 3
}

#[test]
fn test_reverse_nibble() {
    assert_eq!(0x8, reverse_nibble(0x1));
    assert_eq!(0x4, reverse_nibble(0x2));
    assert_eq!(0x2, reverse_nibble(0x4));
    assert_eq!(0x1, reverse_nibble(0x8));
    assert_eq!(0x7, reverse_nibble(0xe));
}

#[repr(u8)]
#[derive(Copy, Clone)]
enum FanState21 {
    Off = 0x3,
    Low = 0x0,
    Med = 0x1,
    High = 0x2,
}

#[derive(Clone, Debug, PartialEq)]
struct FanPkt21 {
    data0: u8,
    data1: u8,
    chksum: u8,
}

impl FanPkt21 {
    fn new(addr: u8, brightness: f64, fan: FanState21) -> Self {
        const BRIGHTNESS_MAX: u8 = 62;
        // Fan seems to reject commands with brightness < ~30%
        const BRIGHTNESS_MIN: u8 = 19;
        assert!(brightness >= 0.0 && brightness <= 1.0);
        // Scale brightness.
        let brightness = if brightness == 0.0 {
            // Max value indicates off
            63
        } else {
            ((BRIGHTNESS_MAX - BRIGHTNESS_MIN) as f64 * brightness) as u8 + BRIGHTNESS_MIN
        };
        let data0 = 0x7 << 5 | reverse_nibble(addr) << 1 | 1;
        let data1 = brightness << 2 | fan as u8;
        let chksum = (data0 >> 4) + (data0 & 0xf) + (data1 >> 4) + (data1 & 0xf) + 3;
        FanPkt21 {
            data0: data0,
            data1: data1,
            chksum: chksum & 0xf,
        }
    }
}

impl<'a> IntoIterator for &'a FanPkt21 {
    type Item = bool;
    type IntoIter = FanPkt21Bits<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FanPkt21Bits::new(self)
    }
}

#[derive(Clone)]
struct FanPkt21Bits<'a> {
    pkt: &'a FanPkt21,
    count: u8,
}

impl<'a> FanPkt21Bits<'a> {
    fn new(pkt: &'a FanPkt21) -> Self {
        FanPkt21Bits {
            pkt: pkt,
            count: 0,
        }
    }
}

impl<'a> Iterator for FanPkt21Bits<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.count {
            0...7 => Some(self.pkt.data0 & (1 << (7 - (self.count - 0))) != 0),
            8...15 => Some(self.pkt.data1 & (1 << (7 - (self.count - 8))) != 0),
            16 => Some(true),
            17...20 => Some(self.pkt.chksum & (1 << (3 - (self.count - 17))) != 0),
            _ => return None,
        };
        self.count += 1;
        ret
    }
}

#[test]
fn fan21_serializer() {
    fn from_iter<I: Iterator<Item = bool>>(mut iter: I) -> u8 {
        // Three high bits
        assert_eq!(iter.next().unwrap(), true);
        assert_eq!(iter.next().unwrap(), true);
        assert_eq!(iter.next().unwrap(), true);
        let addr = if iter.next().unwrap() { 1 << 0 } else { 0 } |
                   if iter.next().unwrap() { 1 << 1 } else { 0 } |
                   if iter.next().unwrap() { 1 << 2 } else { 0 } |
                   if iter.next().unwrap() { 1 << 3 } else { 0 };
        // High bit
        assert_eq!(iter.next().unwrap(), true);
        // State
        for _ in 0..8 {
            iter.next().unwrap();
        }
        // High bit
        assert_eq!(iter.next().unwrap(), true);
        // Chksum
        for _ in 0..4 {
            iter.next().unwrap();
        }
        assert!(iter.next().is_none());
        addr
    }
    for addr in 0..16 {
        for state in [FanState21::Off].iter() {
            let pkt = FanPkt21::new(addr, 0.0, *state);
            assert_eq!(addr, from_iter(pkt.into_iter()));
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

fn arg_app<'a, 'b>() -> App<'a, 'b> {
    App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .arg(Arg::with_name("spidev")
            .short("s")
            .long("spidev")
            .help(concat!("Linux spidev device. Defaults to ", SPIDEV_DEFAULT!()))
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
            .help(concat!("Transmit power. Range 0-7. Defaults to ",
                          TX_POWER_DEFAULT!()))
            .takes_value(true))
        .arg(Arg::with_name("verbose")
            .short("v")
            .long("verbose")
            .help("Verbose logging"))
        .arg(Arg::with_name("debug")
            .short("d")
            .long("debug")
            .help("Debug logging (implies debug)"))
        .subcommand(SubCommand::with_name("dumb")
            .about("Send a 12-bit command. For fans with no LCD in the remote where the fan \
                    keeps the dimmer state.")
            .arg(Arg::with_name("command")
                .index(1)
                .required(true)
                .help("light\tToggle the light\n\
                       off\tFan off\n\
                       low\tFan low\n\
                       medum\tFan medium\n\
                       high\tFan high\n")))
        .subcommand(SubCommand::with_name("smart")
            .about("Send a 21-bit command. For fans with an LCD in the remote where the remote \
                    keeps the dimmer state.")
            .arg(Arg::with_name("fan")
                .index(1)
                .required(true)
                .help("off\tFan off\nlow\tFan low\nmedum\tFan medium\nhigh\tFan high\n"))
            .arg(Arg::with_name("brightness")
                .index(2)
                .required(true)
                .help("Light brightness percentage (0-100)")))
        .setting(AppSettings::SubcommandRequired)
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
    let app = arg_app();
    let matches = app.get_matches();
    log_init(&matches);
    let txpower = matches.value_of("txpower")
        .map(|p| p.parse::<u8>().expect("Invalid argument for txpower"))
        .unwrap_or(TX_POWER_DEFAULT!());
    if txpower > 7 {
        panic!("Requested TX power out of range.");
    }

    let pkt = if let Some(matches) = matches.subcommand_matches("dumb") {
        let cmd = match matches.value_of("command").unwrap() {
            "light" => FanCmd12::Light,
            "off" => FanCmd12::FanOff,
            "low" => FanCmd12::FanLow,
            "medium" => FanCmd12::FanMed,
            "high" => FanCmd12::FanHigh,
            _ => {
                clap::Error::with_description("Invalid fan command. Possible values: \
                                               light|off|low|medium|high",
                                              clap::ErrorKind::UnknownArgument)
                    .exit();
            }
        };
        FanPkt::Dumb(FanPkt12::new(0x9, cmd))
    } else if let Some(matches) = matches.subcommand_matches("smart") {
        let fan = match matches.value_of("fan").unwrap() {
            "off" => FanState21::Off,
            "low" => FanState21::Low,
            "medium" => FanState21::Med,
            "high" => FanState21::High,
            _ => {
                clap::Error::with_description("Invalid fan state. Possible values: \
                                               off|low|medium|high",
                                              clap::ErrorKind::UnknownArgument)
                    .exit();
            }
        };
        let brightness = matches.value_of("brightness")
            .unwrap()
            .parse::<u8>()
            .map(|brightness| {
                if brightness > 100 {
                    clap::Error::with_description("Brightness out of range 0-100",
                                                  clap::ErrorKind::ValueValidation)
                        .exit();
                }
                brightness as f64 / 100.0
            })
            .unwrap_or_else(|_| {
                clap::Error::with_description("Unable to parse brightness as integer",
                                              clap::ErrorKind::InvalidValue)
                    .exit();
            });
        FanPkt::Smart(FanPkt21::new(0xe, brightness, fan))
    } else {
        // Arg parser enforces subcommand requirement
        unreachable!()
    };

    let spidev_path = matches.value_of("spidev").unwrap_or(SPIDEV_DEFAULT!());
    let mut rf = if let Ok(mut spi) = Spidev::open(spidev_path) {
        let shutdown = matches.value_of("shutdown")
            .map(|p| Pin::new(p.parse::<u64>().expect("Invalid argument for shutdown")));
        let irq = matches.value_of("irq")
            .map(|p| Pin::new(p.parse::<u64>().expect("Invalid argument for irq")));
        let options = SpidevOptions::new()
            .max_speed_hz(10 * 1000 * 1000)
            .build();
        spi.configure(&options).unwrap();
        Rfm22::new(spi, irq, shutdown)
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
    pkt.transmit(&mut rf);
}
