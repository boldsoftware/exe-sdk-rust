//! The SDK from synchronous code, with the `blocking` feature.
//!
//! EXE_TOKEN=... cargo run --example blocking --features blocking

use exedev::blocking::BlockOn as _;

fn main() -> exedev::Result<()> {
    let exe = exedev::Client::from_env()?;
    for vm in exe.vms().block_on()? {
        println!("{}\t{}\t{}", vm.name(), vm.status(), vm.tags().join(","));
    }
    Ok(())
}
