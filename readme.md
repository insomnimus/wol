# Wol
`wol` is a command line tool that lets you manage the system sound level on Windows.

## Installation
Download a release from the [releases page](https://github.com/insomnimus/wol/releases) or build from source.

## Build Requirements
- Rust toolchain: at least v1.65
- You also need to have a Windows target installed for Rust; e.g. `x86_64-pc-windows-msvc`. The default installation of Rustup on a Windows machine will come with an appropriate target.

## Build Instructions
After downloading the project's source tree, run `cargo build --release` inside the folder. The executable will be written to `target/release/wol.exe`.
You can move the executable to any location you want.

## Usage
```powershell
# See current levels
wol
# Set the default output device's master volume to 50%
wol 50
# Set the left channel volume of the default output to 75%
wol l75
# Increase master volume by 10
wol +10
# Decrease master volume by 10
wol -10
# Increase left channel volume by 15
wol l+15
# Make left and right channels equal
wol l=r
# Set levels for left and right channels in one command
wol l90 r100

# Set the 4th channel's volume to 25%
wol 4=25
# Set all channels to 50%
wol a50
# Decrease all channels by 10
wol a-10
# Set channel 3 to have the same level as master
wol 3=m
# Set channel 0 (left) to have the same level as channel 5
wol 1=c5

# Set the master level of a specific audio output
wol --device speakers 42
# See the available devices
wol --list

# Read the help message
wol --help
```
