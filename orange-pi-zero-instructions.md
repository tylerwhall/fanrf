# Orange Pi Zero setup guide

## Download Armbian

[Armbian for orange-pi-zero](https://www.armbian.com/orange-pi-zero/), mainline kernel: [direct link](https://dl.armbian.com/orangepizero/Ubuntu_xenial_next_nightly.7z)

Burn to SD, connect ethernet. Default hostname is orangepizero.

- Login: root, 1234
 - Do initial password/account setup

## Set up spidev in device tree
- Edit /boot/armbianEnv.txt as root. Add the following two lines:
```
overlays=spi-spidev
param_spidev_spi_bus=1
```
- Reboot and make sure /dev/spidev1.0 exists

## Install Rust
- Lazy mode Rust installation:
`curl https://sh.rustup.rs -sSf | sh`
- Log out and log back in or `source $HOME/.cargo/env`

## Clone and build fanrf
```
git clone https://github.com/tylerwhall/fanrf.git
cd fanrf
cargo build
```
- Wait several minutes... yay Rust compiling on Cortex-A7

## Fan address
Get the 4-bit dip switch setting from the fan remote as a decimal value.

Example:

> 1 on, 2 on, 3 on, 4 off = 0b1110 = 0xe = 14

## Run
`./target/debug/fanrf --help`

There are "dumb" and "smart" subcommands. Smart is for fans with the LCD screen remote.

`./target/debug/fanrf smart --help`

You need the subcommand and its arguments (smart, fan, light), gpio number args (irq=10 and shutdown=7 for Orange Pi RFM22 board), spidev arg, and the address arg. The smart protocol sends both the light and fan state in one RF packet, so you must specify both. Fanrf needs to run as root or have access to /dev/spidev1.0 or it will automatically fall back to the dummy RF backend and do nothing.
> WARN:fanrf: Using dummy backend.

### Smart command example
Medium fan speed, 75% light:

`sudo ./target/debug/fanrf --spidev=/dev/spidev1.0 --irq=10 --shutdown=7 --address=14 smart medium 75`

### Dumb command example
Low fan speed:

`sudo ./target/debug/fanrf --spidev=/dev/spidev1.0 --irq=10 --shutdown=7 --address=9 dumb low`

Output power can be increased with the --power option to get more range.
