use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;
use dbus::blocking::Connection;
use anyhow::Result;


const BPATH: &str = "/sys/class/backlight";
const SPATH: &str = "/sys/bus/iio/devices";
const BDEV:  &str = "intel_backlight";      // TODO: make into dynamic/config
const SDEV:  &str = "iio:device0";          // TODO: make into dynamic/config
const TIMESTEP: u64 = 10;                    // In ms, 

pub struct Config {
    pub dbus_dest: String,
    pub dbus_path: String,
    pub dbus_session: String,
    pub backlight_path: String,
    pub backlight_max_path: String,
    pub backlight_dev: String,
    pub ambient_path: String,
}

impl Config {
    pub fn default() -> Config {
        Config {
            dbus_dest: "org.freedesktop.login1".to_string(),
            dbus_path: "/org/freedesktop/login1/session/auto".to_string(),
            dbus_session: "org.freedesktop.login1.Session".to_string(),
            backlight_path: format!("{BPATH}/{BDEV}/brightness"),
            backlight_max_path: format!("{BPATH}/{BDEV}/max_brightness"),
            backlight_dev: BDEV.to_string(),
            ambient_path: format!("{SPATH}/{SDEV}/in_illuminance_raw")
        }
    }
}

pub struct Backlight {
    pub conn: Connection,
    pub config: Config,
    pub max: i32,
    pub min: i32,
    pub step: i32
}

impl Backlight {
    pub fn new(config: Config) -> Result<Backlight> {
        let max = read_sys_file(&config.backlight_max_path)?;
        let min = max / 20; // 5% of max
        let step = max / 100; // 1% of maximum
        Ok(Backlight {
            conn: Connection::new_system()?,
            config,
            max,
            min,
            step
        })
    }

    pub fn get_brightness(&self) -> Result<i32> {
        read_sys_file(&self.config.backlight_path)
    }

    /// Converts illuminance (in lux) into backlight power.
    /// This needs some (subjectively) serious tuning work done on it.
    ///
    ///     ########################################
    ///     # Environment     # Lux   # Brightness #
    ///     ########################################
    ///     | Dark            | 0     | 5%         |
    ///     +-----------------+-------+----+-------+
    ///     | Inside (dim)    | 100   | 20%?       |
    ///     +-----------------+-------+----+-------+
    ///     | Inside (med)    | 150   | 30%?       |
    ///     +-----------------+-------+----+-------+
    ///     | Inside (bright) | 200   | 40%?       |
    ///     +-----------------+-------+----+-------+
    ///     | Outside         | 500+  | 100%       |
    ///     +-----------------+-------+------------+
    ///
    pub fn from_ambient(&self, ambient: i32) -> i32 {
        (192 * (ambient + 5 - ambient % 5)).clamp(self.min, self.max)
    }

    pub fn set_brightness(&self, ambient: i32, term: Arc<AtomicBool>) -> Result<()> {
        let proxy = self.conn.with_proxy(&self.config.dbus_dest, &self.config.dbus_path, Duration::from_millis(5000));
        let mut brightness = self.get_brightness()?;
        let next_brightness = self.from_ambient(ambient);
        //println!("\t{brightness} -> {next_brightness}\t{}",next_brightness % 192);
        let step = if brightness > next_brightness {
            self.step * -1
        } else {
            self.step
        };
        while !term.load(Ordering::Relaxed) {
            if (brightness - next_brightness).abs() > self.step {
                brightness += step;
                let _ = proxy.method_call(&self.config.dbus_session, "SetBrightness", ("backlight", &self.config.backlight_dev, brightness as u32))?;
                thread::sleep(Duration::from_millis(TIMESTEP));
            } else {
                break;
            }
        }
        Ok(())
    }
}

pub fn read_sys_file(path: &String) -> Result<i32> {
    let mut f = File::open(path)?;
    let mut buf = String::new();
    let _ = f.read_to_string(&mut buf);
    Ok(buf.trim().parse::<i32>().unwrap())
}

