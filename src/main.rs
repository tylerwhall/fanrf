#[macro_use]
extern crate bitflags;
extern crate spidev;

use std::fmt::Debug;
use std::io::{self, Write};
use std::ops::DerefMut;

use spidev::{Spidev, SpidevOptions, SpidevTransfer};

trait RegRw {
    fn read(&mut self, reg: u8) -> io::Result<u8>;
    fn write(&mut self, reg: u8, val: u8) -> io::Result<()>;
}

// Not sure why this is required
impl<T: RegRw + ?Sized> RegRw for Box<T> {
    fn read(&mut self, reg: u8) -> io::Result<u8> {
        self.deref_mut().read(reg)
    }
    fn write(&mut self, reg: u8, val: u8) -> io::Result<()> {
        self.deref_mut().write(reg, val)
    }
}

struct RfmRegs {
    spi: Spidev,
}

impl RfmRegs {
    fn new(spi: Spidev) -> Self {
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
}

struct FakeRegs([u8; 0x80]);

impl FakeRegs {
    fn new() -> Self {
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
}

struct RegLogger<R: RegRw>(pub R);

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
}

trait RfmReg {
    /// Get the register number for this reg struct
    fn regval() -> u8;
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum Rfm22RegVal {
    DataAccessControl = 0x30,
    HeaderControl2 = 0x33,
    OperatingFunctionControl1 = 0x7,
    TxDataRate1 = 0x6e,
    TxDataRate0 = 0x6f,
    ModulationModeControl1 = 0x70,
    ModulationModeControl2 = 0x71,
    FrequencyOffset1 = 0x73,
    FrequencyOffset2 = 0x74,
    FrequencyBandSelect = 0x75,
    CarrierFrequency1 = 0x76,
    CarrierFrequency0 = 0x77,
}

trait Rfm22Reg: Sized + PartialEq + Debug + Copy {
    fn reg() -> Rfm22RegVal;

    fn from_bits(bits: u8) -> Option<Self>;
    fn bits(&self) -> u8;
}

impl<R: Rfm22Reg> RfmReg for R {
    fn regval() -> u8 {
        R::reg() as u8
    }
}

macro_rules! rfreg {
    ( $name:ident { $($flag:ident = $value:expr),+ } ) => {
        bitflags! {
            flags $name: u8 {
                $(const $flag = (1 << $value)),+
            }
        }

        impl Rfm22Reg for $name {
            fn reg() -> Rfm22RegVal {
                Rfm22RegVal::$name
            }

            #[inline(always)]
            fn from_bits(bits: u8) -> Option<Self> {
                Self::from_bits(bits)
            }

            #[inline(always)]
            fn bits(&self) -> u8 {
                self.bits()
            }
        }
    };
}

rfreg! {
    DataAccessControl {
        CRC0 = 0,
        CRC1 = 1,
        ENCRC = 2,
        ENPACTX = 3,
        SKIP2PH = 4,
        CRCDONLY = 5,
        LSBFIRST = 6,
        ENPACRX = 7
    }
}
rfreg! {
    HeaderControl2 {
        PREALEN8 = 0,
        SYNCLEN0 = 1,
        SYNCLEN1 = 2,
        FIXPKLEN = 3,
        HDLEN0 = 4,
        HDLEN1 = 5,
        HDLEN2 = 6,
        SKIPSYN = 7
    }
}
rfreg! {
    OperatingFunctionControl1 {
        XTON = 0,
        PLLON = 1,
        RXON = 2,
        TXON = 3,
        X32KSEL = 4,
        ENWT = 5,
        ENLBD = 6,
        SWRES = 7
    }
}
rfreg! {
    ModulationModeControl1 {
        ENWHITE = 0,
        ENMANCH = 1,
        ENMANINV = 2,
        MANPPOL = 3,
        ENPHPWDN = 4,
        TXDRTSCALE = 5
    }
}
rfreg! {
    ModulationModeControl2 {
        MODTYP0 = 0,
        MODTYP1 = 1,
        FD8 = 2,
        ENINV = 3,
        DTMOD0 = 4,
        DTMOD1 = 5,
        TRCLK0 = 6,
        TRCLK1 = 7
    }
}

pub enum ModulationType {
    Unmodulated,
    OOK,
    FSK,
    GFSK,
}

pub enum DataSource {
    DirectGPIO,
    DirectSDI,
    FIFO,
    PN9,
}

impl ModulationModeControl2 {
    pub fn set_modtype(&mut self, ty: ModulationType) {
        self.remove(MODTYP0 | MODTYP1);
        self.insert(match ty {
            ModulationType::Unmodulated => Self::empty(),
            ModulationType::OOK => MODTYP0,
            ModulationType::FSK => MODTYP1,
            ModulationType::GFSK => MODTYP1 | MODTYP0,
        });
    }

    pub fn set_data_source(&mut self, source: DataSource) {
        self.remove(DTMOD0 | DTMOD1);
        self.insert(match source {
            DataSource::DirectGPIO => Self::empty(),
            DataSource::DirectSDI => DTMOD0,
            DataSource::FIFO => DTMOD1,
            DataSource::PN9 => DTMOD1 | DTMOD0,
        });
    }
}

rfreg! {
    FrequencyBandSelect {
        FB0 = 0,
        FB1 = 1,
        FB2 = 2,
        FB3 = 3,
        FB4 = 4,
        HBSEL = 5,
        SBSEL = 6
    }
}

impl FrequencyBandSelect {
    fn from_band(band: u8) -> Self {
        Self::from_bits(band as u8).unwrap()
    }
}

rfreg! {
    FrequencyOffset1 {
        FO0 = 0,
        FO1 = 1,
        FO2 = 2,
        FO3 = 3,
        FO4 = 4,
        FO5 = 5,
        FO6 = 6,
        FO7 = 7
    }
}

impl FrequencyOffset1 {
    fn from_frequency_offset(val: u16) -> Self {
        Self::from_bits(val as u8).unwrap()
    }
}

rfreg! {
    FrequencyOffset2 {
        FO8 = 0,
        FO9 = 1
    }
}

impl FrequencyOffset2 {
    fn from_frequency_offset(val: u16) -> Self {
        Self::from_bits((val >> 8) as u8).unwrap()
    }
}

rfreg! {
    CarrierFrequency1 {
        FC8 = 0,
        FC9 = 1,
        FC10 = 2,
        FC11 = 3,
        FC12 = 4,
        FC13 = 5,
        FC14 = 6,
        FC15 = 7
    }
}

impl CarrierFrequency1 {
    fn from_carrier(val: u16) -> Self {
        Self::from_bits((val >> 8) as u8).unwrap()
    }
}

rfreg! {
    CarrierFrequency0 {
        FC0 = 0,
        FC1 = 1,
        FC2 = 2,
        FC3 = 3,
        FC4 = 4,
        FC5 = 5,
        FC6 = 6,
        FC7 = 7
    }
}

impl CarrierFrequency0 {
    fn from_carrier(val: u16) -> Self {
        Self::from_bits(val as u8).unwrap()
    }
}

rfreg! {
    TxDataRate1 {
        TXDR8 = 0,
        TXDR9 = 1,
        TXDR10 = 2,
        TXDR11 = 3,
        TXDR12 = 4,
        TXDR13 = 5,
        TXDR14 = 6,
        TXDR15 = 7
    }
}

impl TxDataRate1 {
    fn from_txdr(val: u16) -> Self {
        Self::from_bits((val >> 8) as u8).unwrap()
    }
}

rfreg! {
    TxDataRate0 {
        TXDR0 = 0,
        TXDR1 = 1,
        TXDR2 = 2,
        TXDR3 = 3,
        TXDR4 = 4,
        TXDR5 = 5,
        TXDR6 = 6,
        TXDR7 = 7
    }
}

impl TxDataRate0 {
    fn from_txdr(val: u16) -> Self {
        Self::from_bits(val as u8).unwrap()
    }
}

struct Rfm22 {
    regs: RegLogger<Box<RegRw>>,
}

impl Rfm22 {
    pub fn new(spi: Spidev) -> Self {
        Rfm22 { regs: RegLogger(Box::new(RfmRegs::new(spi))) }
    }

    pub fn dummy() -> Self {
        Rfm22 { regs: RegLogger(Box::new(FakeRegs::new())) }
    }

    fn read<R: Rfm22Reg>(&mut self) -> io::Result<R> {
        self.regs.read(R::regval()).map(|val| R::from_bits(val).unwrap())
    }

    fn write<R: Rfm22Reg>(&mut self, val: R) -> io::Result<()> {
        self.regs.write(R::regval(), val.bits())
    }

    fn modify<R: Rfm22Reg, F>(&mut self, f: F) -> io::Result<()>
        where F: FnOnce(&mut R) {
            let mut val = self.read()?;
            f(&mut val);
            self.write_validate(val)
    }

    fn write_validate<R: Rfm22Reg>(&mut self, val: R) -> io::Result<()> {
        self.write(val)?;
        assert_eq!(val, self.read().unwrap());
        Ok(())
    }

    fn set_modulation_type_and_source(&mut self, ty: ModulationType, source: DataSource) -> io::Result<()> {
        self.modify(|reg: &mut ModulationModeControl2| {
            reg.set_modtype(ty);
            reg.set_data_source(source);
        })
    }

    fn set_freq_mhz(&mut self, freq: f64) -> io::Result<()> {
        let band = (freq as u32 - 240) / 10;
        assert!(band <= 0x1f);

        let mut bandsel = FrequencyBandSelect::from_band(band as u8);
        if freq >= 480.0 {
            bandsel |= HBSEL;
        }

        let foffset = 0;

        let mut fcarrier = freq;
        if bandsel.contains(HBSEL) {
            fcarrier /= 20.0;
        } else {
            fcarrier /= 10.0;
        }
        fcarrier -= (band + 24) as f64;
        fcarrier *= 64000.0;
        println!("Fcarrier {}", fcarrier);
        let fcarrier = fcarrier as u64;
        println!("Fcarrier {}", fcarrier);
        assert!(fcarrier <= 0xffff);

        self.write_validate(bandsel)?;
        self.write_validate(FrequencyOffset1::from_frequency_offset(foffset))?;
        self.write_validate(FrequencyOffset2::from_frequency_offset(foffset))?;
        self.write_validate(CarrierFrequency1::from_carrier(fcarrier as u16))?;
        self.write_validate(CarrierFrequency0::from_carrier(fcarrier as u16))
    }

    fn set_data_rate_hz(&mut self, rate: f64) -> io::Result<()> {
        let scale = rate < 30000.0;
        self.modify(|mc1: &mut ModulationModeControl1| {
            if scale {
                *mc1 |= TXDRTSCALE;
            }
        })?;
        let exp = if scale { 16 + 5 } else { 16 };
        let txdr = rate * (1 << exp) as f64;
        let txdr = (txdr / 1000000.0) as u64;
        assert!(txdr <= 0xffff);
        self.write_validate(TxDataRate1::from_txdr(txdr as u16))?;
        self.write_validate(TxDataRate0::from_txdr(txdr as u16))
    }

    pub fn init(&mut self) {
        self.write_validate(XTON | PLLON).unwrap();
    }
}

fn main() {
    let mut rf = if let Ok(mut spi) = Spidev::open("/dev/spidev1.0") {
        let options = SpidevOptions::new()
            .max_speed_hz(10 * 1000 * 1000)
            .build();
        spi.configure(&options).unwrap();
        Rfm22::new(spi)
    } else {
        println!("Using dummy backend.");
        Rfm22::dummy()
    };

    rf.init();
    rf.set_modulation_type_and_source(ModulationType::OOK, DataSource::FIFO).unwrap();
    rf.write_validate(DataAccessControl::empty()).unwrap();
    // HeaderControl2
    rf.write_validate(SKIPSYN).unwrap();
    rf.set_freq_mhz(303.8).unwrap();
    rf.set_data_rate_hz(3000.0).unwrap();
}
