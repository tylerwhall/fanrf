use std::fmt::Debug;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use spidev::Spidev;
use sysfs_gpio::{Direction, Edge, Pin, PinPoller};

use regrw::{FakeRegs, RegRw, RfmReg, RfmRegs, RegLogger};

const FIFO_SIZE: usize = 64;

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Rfm22RegVal {
    InterruptStatus1 = 0x3,
    InterruptStatus2 = 0x4,
    InterruptEnable1 = 0x5,
    InterruptEnable2 = 0x6,
    OperatingFunctionControl1 = 0x7,
    OperatingFunctionControl2 = 0x8,
    DataAccessControl = 0x30,
    HeaderControl2 = 0x33,
    TxPower = 0x6d,
    TxDataRate1 = 0x6e,
    TxDataRate0 = 0x6f,
    ModulationModeControl1 = 0x70,
    ModulationModeControl2 = 0x71,
    FrequencyOffset1 = 0x73,
    FrequencyOffset2 = 0x74,
    FrequencyBandSelect = 0x75,
    CarrierFrequency1 = 0x76,
    CarrierFrequency0 = 0x77,
    FIFOAccess = 0x7f,
}

pub trait Rfm22Reg: Sized + PartialEq + Debug + Copy {
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
            pub flags $name: u8 {
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

// #[derive(PartialEq)]
// pub enum Interrupt {
// CRCError = 0,
// PkValid,
// PkSent,
// Ext,
// RxFIFOAlmostFull,
// TxFIFOAlmostEmpty,
// TxFIFOAlmostFull,
// FIFOError,
// Por,
// ChipRdy,
// Lbd,
// Wut,
// Rssi,
// Preainval,
// Preaval,
// Swdet,
// }
//

rfreg! {
    InterruptStatus1 {
        ICRCERROR = 0,
        IPKVALID = 1,
        IPKSENT = 2,
        IEXT = 3,
        IRXFFAFULL = 4,
        ITXFFAEM = 5,
        ITXFFAFULL = 6,
        IFFERR = 7
    }
}
rfreg! {
    InterruptStatus2 {
        IPOR = 0,
        ICHIPRDY = 1,
        ILBDET = 2,
        IWUT = 3,
        IRSSI = 4,
        IPREAINVAL = 5,
        IPREAVAL = 6,
        ISWDET = 7
    }
}
rfreg! {
    InterruptEnable1 {
        ENCRCERROR = 0,
        ENPKVALID = 1,
        ENPKSENT = 2,
        ENEXT = 3,
        ENRXFFAFULL = 4,
        ENTXFFAEM = 5,
        ENTXFFAFULL = 6,
        ENFFERR = 7
    }
}

impl From<InterruptEnable1> for InterruptStatus1 {
    fn from(val: InterruptEnable1) -> Self {
        InterruptStatus1::from_bits_truncate(val.bits())
    }
}

rfreg! {
    InterruptEnable2 {
        ENPOR = 0,
        ENCHIPRDY = 1,
        ENLBDET = 2,
        ENWUT = 3,
        ENRSSI = 4,
        ENPREAINVAL = 5,
        ENPREAVAL = 6,
        ENSWDET = 7
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
    OperatingFunctionControl2 {
        FFCLRTX = 0,
        FFCLRRX = 1,
        ENLDM = 2,
        AUTOTX = 3,
        RXMPK = 4,
        ANTDIV0 = 5,
        ANTDIV1 = 6,
        ANTDIV2 = 7
    }
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
    TxPower {
        TXPOW0 = 0,
        TXPOW1 = 1,
        TXPOW2 = 2,
        LNA_SW = 3,
        PAPEAKLV0 = 4,
        PAPEAKLV1 = 5,
        PAPEAKEN = 6,
        PAPEAKVAL = 7
    }
}

impl TxPower {
    fn set_tx_power(&mut self, power: u8) {
        assert!(power <= 0x7);
        self.remove(TXPOW2 | TXPOW1 | TXPOW0);
        self.insert(Self::from_bits(power).unwrap());
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

#[allow(unused)]
pub enum ModulationType {
    Unmodulated,
    OOK,
    FSK,
    GFSK,
}

#[allow(unused)]
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

pub struct Rfm22Regs {
    regs: RegLogger<Box<RegRw>>,
}

impl Rfm22Regs {
    pub fn new(spi: Spidev) -> Self {
        Rfm22Regs { regs: RegLogger(Box::new(RfmRegs::new(spi))) }
    }

    pub fn dummy() -> Self {
        Rfm22Regs { regs: RegLogger(Box::new(FakeRegs::new())) }
    }

    pub fn read<R: Rfm22Reg>(&mut self) -> io::Result<R> {
        self.regs.read(R::regval()).map(|val| R::from_bits(val).unwrap())
    }

    pub fn write<R: Rfm22Reg>(&mut self, val: R) -> io::Result<()> {
        self.regs.write(R::regval(), val.bits())
    }

    pub fn modify<R: Rfm22Reg, F>(&mut self, f: F) -> io::Result<()>
        where F: FnOnce(&mut R)
    {
        let mut val = self.read()?;
        f(&mut val);
        self.write(val)
    }

    pub fn modify_verify<R: Rfm22Reg, F>(&mut self, f: F) -> io::Result<()>
        where F: FnOnce(&mut R)
    {
        let mut val = self.read()?;
        f(&mut val);
        self.write_validate(val)
    }

    pub fn write_validate<R: Rfm22Reg>(&mut self, val: R) -> io::Result<()> {
        self.write(val)?;
        assert_eq!(val, self.read().unwrap());
        Ok(())
    }

    pub fn burst_write(&mut self, reg: Rfm22RegVal, buf: &[u8]) -> io::Result<()> {
        self.regs.burst_write(reg as u8, buf)
    }
}

struct Rfm22IRQs {
    pending: InterruptStatus1,
    enabled: InterruptEnable1,
    gpio_poller: Option<(Pin, PinPoller)>,
    dummy: bool,
}

impl Rfm22IRQs {
    fn new(mut gpio: Option<Pin>) -> Self {
        if let Some(ref mut pin) = gpio {
            pin.set_edge(Edge::FallingEdge).unwrap();
        }
        Rfm22IRQs {
            pending: InterruptStatus1::empty(),
            enabled: InterruptEnable1::empty(),
            gpio_poller: gpio.map(|pin| {
                let poller = pin.get_poller().unwrap();
                (pin, poller)
            }),
            dummy: false,
        }
    }

    fn dummy() -> Self {
        Rfm22IRQs {
            pending: InterruptStatus1::empty(),
            enabled: InterruptEnable1::empty(),
            gpio_poller: None,
            dummy: true,
        }
    }

    /// Returns all IRQs currently pending
    fn poll(&mut self, regs: &mut Rfm22Regs) -> io::Result<InterruptStatus1> {
        // Add new IRQs to the current pending set. Reading enabled IRQs clears
        // them, so we need to remember what we've observed until we mark them
        // as handled.
        self.pending.insert(regs.read()?);
        self.pending &= self.enabled.into();
        if self.dummy {
            return Ok(ITXFFAEM | IPKSENT);
        } else {
            Ok(self.pending)
        }
    }

    fn _wait_for_change(&mut self) {
        if let Some((ref mut pin, ref mut poller)) = self.gpio_poller {
            if pin.get_value().unwrap() > 0 {
                debug!("Poll started");
                match poller.poll(1000).unwrap() {
                    Some(_) => debug!("Poll finished"),
                    None => debug!("Timed out: {}", pin.get_value().unwrap()),
                }
            }
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn wait(&mut self,
            regs: &mut Rfm22Regs,
            irqs: InterruptStatus1)
            -> io::Result<InterruptStatus1> {
        debug!("waiting for {:?}", irqs);
        let mut pnd = self.poll(regs)?;
        debug!("pending {:?}", pnd);
        let start = Instant::now();
        // TODO: should create IRQ poller here to avoid looping on prior events
        while !pnd.contains(irqs) {
            if Instant::now().duration_since(start) > Duration::from_secs(1) {
                error!("Timed out");
                return Err(io::Error::new(io::ErrorKind::TimedOut, "IRQ polling timed out"));
            }
            self._wait_for_change();
            pnd = self.poll(regs)?;
            debug!("pending {:?}", pnd);
        }
        Ok(irqs)
    }

    fn handled(&mut self, irqs: InterruptStatus1) {
        self.pending.remove(irqs)
    }

    /// Clears all enabled IRQs in hardware and clears all considered pending
    fn clear(&mut self, regs: &mut Rfm22Regs) -> io::Result<()> {
        self.poll(regs).map(|pnd| self.handled(pnd))
    }

    fn set_enable(&mut self, regs: &mut Rfm22Regs, irqs: InterruptEnable1) -> io::Result<()> {
        self.enabled = irqs;
        // Clear pending that are not enabled
        let mut toclear = InterruptStatus1::all();
        toclear.remove(irqs.into());
        self.pending.remove(toclear);

        regs.write_validate(irqs)?;
        regs.write_validate(InterruptEnable2::empty())
    }
}

pub struct Rfm22 {
    pub regs: Rfm22Regs,
    irq: Rfm22IRQs,
    shutdown: Option<Pin>,
}

impl Rfm22 {
    pub fn new(spi: Spidev, mut irq: Option<Pin>, mut shutdown: Option<Pin>) -> Self {
        if let Some(ref mut sdn) = shutdown {
            sdn.export().unwrap();
            // Put in reset if not already
            let in_reset = match sdn.get_direction().unwrap() {
                Direction::High => true,
                Direction::Out => sdn.get_value().unwrap() > 0,
                _ => false,
            };
            if !in_reset {
                debug!("Resetting");
                sdn.set_direction(Direction::High).unwrap();
                thread::sleep(Duration::from_millis(1));
            } else {
                debug!("Already in reset");
            }
            // Bring out of reset
            sdn.set_direction(Direction::Low).unwrap();
            // 16.8ms specified from shutdown to TX
            // 20 does not work
            // 30 works
            // Using 40 for margin
            // Should wait on IRQ
            thread::sleep(Duration::from_millis(40));
            info!("Reset complete");
        }
        if let Some(ref mut irq) = irq {
            irq.export().unwrap();
        }
        Rfm22 {
            regs: Rfm22Regs::new(spi),
            irq: Rfm22IRQs::new(irq),
            shutdown: shutdown,
        }
    }

    pub fn dummy() -> Self {
        Rfm22 {
            regs: Rfm22Regs::dummy(),
            irq: Rfm22IRQs::dummy(),
            shutdown: None,
        }
    }

    pub fn set_modulation_type_and_source(&mut self,
                                          ty: ModulationType,
                                          source: DataSource)
                                          -> io::Result<()> {
        self.regs.modify_verify(|reg: &mut ModulationModeControl2| {
            reg.set_modtype(ty);
            reg.set_data_source(source);
        })
    }

    pub fn set_tx_power(&mut self, power: u8) -> io::Result<()> {
        self.regs.modify_verify(|reg: &mut TxPower| reg.set_tx_power(power))
    }

    pub fn set_freq_mhz(&mut self, freq: f64) -> io::Result<()> {
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
        let fcarrier = fcarrier as u64;
        debug!("Fcarrier {}", fcarrier);
        assert!(fcarrier <= 0xffff);

        self.regs.write_validate(bandsel)?;
        self.regs.write_validate(FrequencyOffset1::from_frequency_offset(foffset))?;
        self.regs.write_validate(FrequencyOffset2::from_frequency_offset(foffset))?;
        self.regs.write_validate(CarrierFrequency1::from_carrier(fcarrier as u16))?;
        self.regs.write_validate(CarrierFrequency0::from_carrier(fcarrier as u16))
    }

    pub fn set_data_rate_hz(&mut self, rate: f64) -> io::Result<()> {
        let scale = rate < 30000.0;
        self.regs
            .modify_verify(|mc1: &mut ModulationModeControl1| {
                if scale {
                    *mc1 |= TXDRTSCALE;
                }
            })?;
        let exp = if scale { 16 + 5 } else { 16 };
        let txdr = rate * (1 << exp) as f64;
        let txdr = (txdr / 1000000.0) as u64;
        assert!(txdr <= 0xffff);
        self.regs.write_validate(TxDataRate1::from_txdr(txdr as u16))?;
        self.regs.write_validate(TxDataRate0::from_txdr(txdr as u16))
    }

    fn clear_tx_fifo(&mut self) -> io::Result<()> {
        self.regs
            .modify_verify(|reg: &mut OperatingFunctionControl2| {
                reg.insert(FFCLRTX);
            })?;
        self.regs.modify_verify(|reg: &mut OperatingFunctionControl2| {
            reg.remove(FFCLRTX);
        })
    }

    fn write_tx_fifo(&mut self, buf: &[u8]) -> io::Result<()> {
        self.regs.burst_write(Rfm22RegVal::FIFOAccess, buf)
    }

    fn transmit(&mut self) -> io::Result<()> {
        self.regs.modify(|reg: &mut OperatingFunctionControl1| reg.insert(TXON))
    }

    fn transmit_large<'a, I: IntoIterator<Item = u8>>(&mut self, iter: I) -> io::Result<()> {
        // The almost empty IRQ happens at 4 by default. Leave some extra space
        // so we can never fill the FIFO completely. This could probably be
        // exactly 4, but I don't know how the boundary conditions work in HW.
        let mut buf = Vec::with_capacity(FIFO_SIZE - 10);
        let capacity = buf.capacity();
        let mut iter = iter.into_iter().peekable();

        buf.extend(iter.by_ref().take(capacity));
        if buf.len() == 0 {
            error!("Zero length transmit!");
            return Ok(());
        }
        self.clear_tx_fifo()?;
        self.irq.set_enable(&mut self.regs, ENPKSENT | ENTXFFAEM)?;
        // Clear pending IRQs
        self.irq.clear(&mut self.regs)?;

        // Write initial data
        self.write_tx_fifo(&buf)?;
        // Start transmitter
        self.transmit()?;
        while let Some(_) = iter.peek() {
            self.irq.wait(&mut self.regs, ITXFFAEM)?;
            self.irq.handled(ITXFFAEM);
            buf.clear();
            buf.extend(iter.by_ref().take(capacity));
            self.write_tx_fifo(&buf)?;
        }
        self.irq.wait(&mut self.regs, IPKSENT)?;
        self.irq.handled(IPKSENT);
        Ok(())
    }

    pub fn transmit_bitstream<'a, I: IntoIterator<Item = bool>>(&mut self,
                                                                iter: I)
                                                                -> io::Result<()> {
        struct BitsToBytes<I: Iterator<Item = bool>>(I);

        impl<I: Iterator<Item = bool>> Iterator for BitsToBytes<I> {
            type Item = u8;

            fn next(&mut self) -> Option<Self::Item> {
                let mut val = 0;
                if let Some(bit) = self.0.next() {
                    if bit {
                        val |= 1 << 7;
                    }
                } else {
                    return None;
                }
                // Finish the byte if there was at least 1 bit
                for idx in (0..7).into_iter().rev() {
                    if let Some(bit) = self.0.next() {
                        if bit {
                            val |= 1 << idx;
                        }
                    }
                }
                Some(val)
            }
        }

        self.transmit_large(BitsToBytes(iter.into_iter()))
    }

    pub fn init(&mut self) {
        self.regs.write_validate(XTON | PLLON).unwrap();
    }
}

impl Drop for Rfm22 {
    fn drop(&mut self) {
        // Put in reset when no longer in use
        if let Some(ref mut sdn) = self.shutdown {
            sdn.set_value(1).unwrap();
        }
    }
}
