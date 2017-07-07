use Result;
use clap::ArgMatches;
use config::Config;
use printer::Printer;
use std::io::Write;
use device;
use builder;
use loader;
use debugger;
use console;

pub fn list(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let filter = device::filter(cfg, args, cmd_args);
    let devices = device::search(&filter);

    writeln!(out, "{:08} {:04}:{:04} {:24} {:32} {:24}", 
        "ID",
        "VID", 
        "PID", 
        "Vendor", 
        "Product",
        "Serial Number",
        )?;        
    for d in devices?.iter() {
        let u = d.usb();
        write!(out, "{:08} {:04x}:{:04x} {:24} {:32} {:24}",
            &d.hash()[..8],
            u.vendor_id, 
            u.product_id, 
            u.vendor_string, 
            u.product_string,
            u.serial_number,
            )?;
        writeln!(out, "")?;
    }
    Ok(())
}


pub fn info(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let filter = device::filter(cfg, args, cmd_args);
    let devices = device::search(&filter)?;

    for d in devices.iter() {
        let u = d.usb();        
        writeln!(out, "{:16} {}", "ID", d.hash())?;
        writeln!(out, "{:16} {:04x}", "Vendor ID", u.vendor_id)?;
        writeln!(out, "{:16} {:04x}", "Product ID", u.product_id)?;
        writeln!(out, "{:16} {}", "Vendor", u.vendor_string)?;
        writeln!(out, "{:16} {}", "Product", u.product_string)?;
        writeln!(out, "{:16} {}", "Serial Number", u.serial_number)?;
        writeln!(out, "{:16} {}", "Type", d.device_type().unwrap_or("Unknown"))?;
        if let Some(loader_type) = d.loader_type() {
            writeln!(out, "{:16} {}", "Loader Type", loader_type)?;
        }
        if let Some(debugger_type) = d.debugger_type() {
            writeln!(out, "{:16} {}", "Debugger Type", debugger_type)?;
        }        

        if let Some(bossa_path) = d.bossa_path() {
            writeln!(out, "{:16} {}", "Bossac Device", bossa_path)?;
        }        
        if let Some(cdc_path) = d.cdc_path() {
            writeln!(out, "{:16} {}", "CDC Device", cdc_path)?;
        }
        if let Some(msd_path) = d.msd_path() {
            writeln!(out, "{:16} {}", "MSD Device", msd_path.display())?;
        }
        if let Some(openocd_serial) = d.openocd_serial() {
            writeln!(out, "{:16} {}", "OpenOCD Serial", openocd_serial)?;
        }
        writeln!(out, "")?;
    }
    Ok(())
}

pub fn build(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let dst = builder::build(cfg, args, args, out)?;
    Ok(())
}

pub fn load(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };
    
    let ldr = if let Some(ldr) = device.loader_type() {
        out.verbose("loader",ldr)?;
        if let Some(ldr) = loader::loader(ldr) {
            ldr
        } else {
            bail!("Unknown loader type: {}", ldr);
        }
    } else {
        bail!("Selected device has no associated loader");
    };

    let dst = if let Some(dst) = builder::build(cfg, args, cmd_args, out)? {
        dst
    } else {
        bail!("No build output available to load");
    };
    out.verbose("target", &format!("{}", dst.display()))?;
    
    let con = if !cmd_args.is_present("noconsole") && !cmd_args.is_present("itm") {
        if args.is_present("run") {
            if let Some(cdc_path) = device.cdc_path() {
                let mut con = console::open(&cdc_path)?;
                con.clear()?;
                Some(con)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    ldr.load(cfg, args, cmd_args, out, device.as_ref(), dst.as_path())?;

    out.info("Loader", "Load Complete")?;

    if cmd_args.is_present("itm") {
        if device.can_trace_itm() {
            out.info("ITM", "Starting ITM Trace")?;
            let target_clk = if let Some(v) = cmd_args.value_of("itm-target-clock") {
                v.parse::<u32>()?
            } else {
                if let Some(v) = cfg.itm_target_clock() {
                    v
                } else {
                    bail!("itm-target-clock is required for ITM trace.")
                }
            };
            let trace_clk = 2_000_000;
            device.trace_itm(target_clk, trace_clk)?;

        } else {
            bail!("Currently selected device does not support ITM trace");
        }
    } else if let Some(mut con) = con {
        out.info("Console", "Opening Console")?;
        if cmd_args.is_present("packet") {
            con.view_packet()?;
        } else {
            con.view()?;
        }
    }

    Ok(())
}

pub fn control(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {    
    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };

    let dbg = if let Some(dbg) = device.debugger_type() {
        out.verbose("debugger",dbg)?;
        if let Some(dbg) = debugger::debugger(dbg) {
            dbg
        } else {
            bail!("Unknown debugger type: {}", dbg);
        }
    } else {
        bail!("Selected device has no associated loader");
    };
    
    if let Some(_) = args.subcommand_matches("halt") {
        dbg.halt(cfg, args, cmd_args, out, device.as_ref())?;
    } else if let Some(_) = args.subcommand_matches("resume") {
        dbg.resume(cfg, args, cmd_args, out, device.as_ref())?;
    } else if let Some(_) = args.subcommand_matches("reset") {
        if cmd_args.is_present("run") {
            dbg.reset_run(cfg, args, cmd_args, out, device.as_ref())?;
        } else if cmd_args.is_present("halt") {
            dbg.reset_halt(cfg, args, cmd_args, out, device.as_ref())?;
        } else if cmd_args.is_present("init") {
            dbg.reset_init(cfg, args, cmd_args, out, device.as_ref())?;
        } else {
            dbg.reset(cfg, args, cmd_args, out, device.as_ref())?;
        }
    }

    Ok(())
}

pub fn openocd(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    use std::process::*;
    use std::os::unix::process::CommandExt;

    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };    

    let mut cmd = Command::new("openocd");
    cmd.arg("--file").arg("openocd.cfg");
    cmd.arg("--command").arg(&device.openocd_serial().unwrap());

    cmd.exec();

    let status = cmd.status()?;
    if !status.success() {
        bail!("openocd failed")
    }
    Ok(())
}

pub fn gdb(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    use std::process::*;
    use std::os::unix::process::CommandExt;

    let dst = if let Some(dst) = builder::build(cfg, args, cmd_args, out)? {
        dst
    } else {
        bail!("No build output available for gdb");
    };

    let mut cmd = Command::new("arm-none-eabi-gdb");
    cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg(dst);
    out.verbose("gdb",&format!("{:?}", cmd))?;

    cmd.exec();

    let status = cmd.status()?;
    if !status.success() {
        bail!("gdb failed")
    }
    Ok(())
}

pub fn console(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };    

    if let Some(cdc_path) = device.cdc_path() {
        let mut con = console::open(&cdc_path)?;
        con.view()?
    } else {
        bail!("No console found for device");
    }
    
    Ok(())    
}

pub fn screen(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    use std::process::*;
    use std::os::unix::process::CommandExt;

    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };    

    let mut cmd = Command::new("screen");
    if let Some(cdc_path) = device.cdc_path() {
        cmd.arg(cdc_path);
    } else {
        bail!("No serial device path found");
    }
    cmd.arg("115200");
    cmd.exec();

    let status = cmd.status()?;
    if !status.success() {
        bail!("screen failed")
    }
    Ok(())
}

pub fn objdump(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    Ok(())
}

pub fn itm(cfg: &Config, args: &ArgMatches, cmd_args: &ArgMatches, out: &mut Printer) -> Result<()> {
    let filter = device::filter(cfg, args, cmd_args);
    let mut devices = device::search(&filter)?;

    let device = if devices.len() == 0 {
        bail!("No matching devices found.");
    } else if devices.len() > 1 {
        bail!("More than one device found ({})", devices.len());
    } else {
        devices.remove(0)
    };    

    if device.can_trace_itm() {
        out.info("ITM", "Starting ITM Trace")?;
        let target_clk = if let Some(v) = cmd_args.value_of("itm-target-clock") {
            v.parse::<u32>()?
        } else {
            if let Some(v) = cfg.itm_target_clock() {
                v
            } else {
                bail!("itm-target-clock is required for ITM trace.")
            }
        };
        let trace_clk = 2_000_000;
        device.trace_itm(target_clk, trace_clk)?;
    } else {
        bail!("Currently selected device does not support ITM trace");
    }
    Ok(())
}