#[macro_use]
extern crate log;

use anyhow::Result;
use std::{
    convert::{TryFrom, TryInto},
    error::Error,
    ffi::CStr,
    fmt::{self, Display},
    fs::File,
    io::{self, Read},
    mem::MaybeUninit,
    os::raw::c_int,
    path::Path,
    ptr,
    str::FromStr,
    thread,
    time::Duration,
};

#[derive(Debug)]
struct IpmiError(c_int);

impl IpmiError {
    pub fn from_context(context: libfreeipmi_sys::ipmi_ctx_t) -> Self {
        unsafe { Self(libfreeipmi_sys::ipmi_ctx_errnum(context)) }
    }
}

impl Error for IpmiError {}

impl Display for IpmiError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        unsafe {
            write!(
                fmt,
                "{}",
                CStr::from_ptr(libfreeipmi_sys::ipmi_ctx_strerror(self.0))
                    .to_str()
                    .unwrap()
            )
        }
    }
}

#[derive(Debug)]
struct FiidError(libfreeipmi_sys::fiid_err_t);

impl FiidError {
    pub fn from_context(context: libfreeipmi_sys::fiid_obj_t) -> Self {
        unsafe { Self(libfreeipmi_sys::fiid_obj_errnum(context)) }
    }
}

impl Error for FiidError {}

impl Display for FiidError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        unsafe {
            write!(
                fmt,
                "{}",
                CStr::from_ptr(libfreeipmi_sys::fiid_strerror(self.0))
                    .to_str()
                    .unwrap()
            )
        }
    }
}

struct Ipmi {
    context: libfreeipmi_sys::ipmi_ctx_t,
}

impl Ipmi {
    pub fn find_inband() -> Result<Self> {
        unsafe {
            let context = libfreeipmi_sys::ipmi_ctx_create();

            if context.is_null() {
                panic!("ipmi context malloc failed");
            }

            if -1
                == libfreeipmi_sys::ipmi_ctx_find_inband(
                    context,
                    ptr::null_mut(),
                    0,
                    0,
                    0,
                    ptr::null(),
                    0,
                    0,
                )
            {
                libfreeipmi_sys::ipmi_ctx_destroy(context);
                return Err(IpmiError::from_context(context).into());
            }

            Ok(Self { context })
        }
    }

    pub fn set_fan_to_full(&mut self) -> Result<()> {
        info!("Setting fans to full speed");

        unsafe {
            let command = [0x45, 0x01, 0x01];
            let mut resp = [0u8; 8];
            if -1
                == libfreeipmi_sys::ipmi_cmd_raw(
                    self.context,
                    0,
                    0x30,
                    command.as_ptr() as *const _,
                    command.len().try_into().unwrap(),
                    resp.as_mut_ptr() as *mut _,
                    resp.len().try_into().unwrap(),
                )
            {
                return Err(IpmiError::from_context(self.context).into());
            }
        }

        Ok(())
    }

    pub fn set_fan_duty(&mut self, zone: u8, duty: u8) -> Result<(), IpmiError> {
        info!("Setting zone {} fans to {}% duty", zone, duty);

        unsafe {
            let command = [0x70, 0x66, 0x01, zone, duty];
            let mut resp = [0u8; 8];
            if -1
                == libfreeipmi_sys::ipmi_cmd_raw(
                    self.context,
                    0,
                    0x30,
                    command.as_ptr() as *const _,
                    command.len().try_into().unwrap(),
                    resp.as_mut_ptr() as *mut _,
                    resp.len().try_into().unwrap(),
                )
            {
                return Err(IpmiError::from_context(self.context));
            }
        }

        Ok(())
    }

    pub fn read_fan_speed(&mut self) -> Result<[u64; 8]> {
        let mut result = [0u64; 8];

        for fan in 0..8 {
            unsafe {
                let obj = FiidObj::new(&libfreeipmi_sys::tmpl_cmd_get_sensor_reading_rs)?;
                if -1
                    == libfreeipmi_sys::ipmi_cmd_get_sensor_reading(
                        self.context,
                        0x41 + fan,
                        obj.inner,
                    )
                {
                    return Err(IpmiError::from_context(self.context).into());
                }
                let rpm = obj.get(CStr::from_bytes_with_nul_unchecked(b"sensor_reading\0"))? * 100;
                info!("Found fan at {} speed to be {} RPM", fan, rpm);
                result[fan as usize] = rpm;
            }
        }

        Ok(result)
    }

    pub fn reset_bmc(&mut self) -> Result<()> {
        info!("Performing cold reset of BMC");

        unsafe {
            let command = [0x02];
            let mut resp = [0u8; 8];
            if -1
                == libfreeipmi_sys::ipmi_cmd_raw(
                    self.context,
                    0,
                    0x06,
                    command.as_ptr() as *const _,
                    command.len().try_into().unwrap(),
                    resp.as_mut_ptr() as *mut _,
                    resp.len().try_into().unwrap(),
                )
            {
                return Err(IpmiError::from_context(self.context).into());
            }
        }

        Ok(())
    }
}

impl Drop for Ipmi {
    fn drop(&mut self) {
        unsafe {
            libfreeipmi_sys::ipmi_ctx_close(self.context);
            libfreeipmi_sys::ipmi_ctx_destroy(self.context);
        }
    }
}

#[derive(Debug)]
struct FiidObj {
    inner: libfreeipmi_sys::fiid_obj_t,
}

impl FiidObj {
    pub fn new(template: &libfreeipmi_sys::fiid_template_t) -> Result<FiidObj> {
        unsafe {
            let inner = libfreeipmi_sys::fiid_obj_create(template.as_ptr() as *mut _);
            if inner.is_null() {
                Err(io::Error::last_os_error().into())
            } else {
                Ok(Self { inner })
            }
        }
    }

    pub fn get(&self, field: &CStr) -> Result<u64> {
        unsafe {
            let mut value = MaybeUninit::uninit();
            if 0 > libfreeipmi_sys::fiid_obj_get(self.inner, field.as_ptr(), value.as_mut_ptr()) {
                Err(FiidError::from_context(self.inner).into())
            } else {
                Ok(value.assume_init())
            }
        }
    }
}

impl Drop for FiidObj {
    fn drop(&mut self) {
        unsafe {
            libfreeipmi_sys::fiid_obj_destroy(self.inner);
        }
    }
}

// less than 20°C is probably a sensor malfunction
const MIN_TEMP: i32 = 20 * 1000;

fn read_temperature_path<P>(path: P) -> Result<i32, io::Error>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let mut buf = [0u8; 16];
    let mut file = File::open(path)?;
    let len = file.read(&mut buf)?;
    let end = buf[..len]
        .iter()
        .position(|c| *c as char == '\n')
        .unwrap_or(len);
    let s = std::str::from_utf8(&buf[..end])
        .or_else(|_| Err(io::Error::from(io::ErrorKind::InvalidData)))?;
    let i = i32::from_str(s).or_else(|_| Err(io::Error::from(io::ErrorKind::InvalidData)))?;
    if i < MIN_TEMP {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    info!("Found temperature at path {:?} to be {} m°C", path, i);
    Ok(i)
}

fn read_temperature() -> Result<i32, io::Error> {
    let temp0 = read_temperature_path("/sys/class/thermal/thermal_zone0/temp")?;
    let temp1 = read_temperature_path("/sys/class/thermal/thermal_zone1/temp")?;
    Ok((temp0 + temp1) / 2)
}

const MAX_RATE: u8 = 100;
const CURVE_START_LEVEL: u8 = 15;
const CURVE_END_LEVEL: u8 = MAX_RATE;

fn fan_curve(temp: i32) -> u8 {
    const CURVE_START_TEMP: i32 = 35 * 1000;
    const CURVE_END_TEMP: i32 = 65 * 1000;

    let unclamped = i32::from(CURVE_START_LEVEL)
        + i32::from(CURVE_END_LEVEL - CURVE_START_LEVEL) * (temp - CURVE_START_TEMP)
            / (CURVE_END_TEMP - CURVE_START_TEMP);

    if unclamped < 0 {
        0
    } else if unclamped > 255 {
        255
    } else {
        unclamped as u8
    }
}

#[derive(Default)]
struct RateHistory {
    rates: [u8; 5],
    start: u8,
    len: u8,
}

impl RateHistory {
    pub fn push(&mut self, rate: u8) {
        if self.len == self.rates.len() as u8 {
            self.rates[self.start as usize] = rate;
            self.start = (self.start + 1) % self.rates.len() as u8;
        } else {
            self.rates[self.len as usize] = rate;
            self.len += 1;
        }
    }

    pub fn rates(&self) -> Rates<'_> {
        Rates {
            rates: &self.rates,
            index: self.start,
            len: self.len,
        }
    }

    pub fn full(&self) -> bool {
        self.len == self.rates.len() as u8
    }
}

struct Rates<'a> {
    rates: &'a [u8; 5],
    index: u8,
    len: u8,
}

impl<'a> Iterator for Rates<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        if self.len == 0 {
            None
        } else {
            let value = self.rates[self.index as usize];
            self.len -= 1;
            self.index = (self.index + 1) % self.rates.len() as u8;
            Some(value)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len as usize, Some(self.len as usize))
    }
}

impl<'a> ExactSizeIterator for Rates<'a> {}

const UP_DRAG: i16 = 4;
const DOWN_DRAG: i16 = 8;

fn main() {
    env_logger::init();

    let mut ipmi = Ipmi::find_inband().expect("failed to open ipmi");

    ipmi.set_fan_to_full()
        .expect("failed to set fan speed to full");

    let mut history = RateHistory::default();
    let mut reset_lockout = 0u8;
    loop {
        let temperature = read_temperature();
        let rate = temperature.map(fan_curve).unwrap_or(255);
        let rate = if let Some(last_rate) = history.rates().last() {
            let diff = i16::from(rate) - i16::from(last_rate);
            if diff > UP_DRAG {
                debug!("{} > {}", diff, UP_DRAG);
                u8::try_from(i16::from(last_rate) + diff - UP_DRAG).unwrap()
            } else if diff < -DOWN_DRAG {
                debug!("{} < -{}", diff, DOWN_DRAG);
                u8::try_from(i16::from(last_rate) + diff + DOWN_DRAG).unwrap()
            } else {
                debug!("{} is close to {}", rate, last_rate);
                last_rate
            }
        } else {
            debug!("no previous rate for comparison");
            rate
        };
        let rate = if rate < CURVE_START_LEVEL {
            CURVE_START_LEVEL
        } else if rate > CURVE_END_LEVEL {
            CURVE_END_LEVEL
        } else {
            rate
        };
        if let Err(error) = ipmi.set_fan_duty(0, rate) {
            error!("Failed to set zone 0 fan duty cycle: {}", error);
        }
        if let Err(error) = ipmi.set_fan_duty(1, rate) {
            error!("Failed to set zone 1 fan duty cycle: {}", error);
        }
        history.push(rate);

        if history.full() && reset_lockout == 0 {
            let actual_speed = ipmi.read_fan_speed().unwrap_or([0u64; 8]);

            if (history.rates().min().unwrap() > 80 && *actual_speed.iter().max().unwrap() < 10_000)
                || (history.rates().max().unwrap() < 50
                    && *actual_speed.iter().min().unwrap() > 10_000)
            {
                reset_lockout = 15;
                let _ = ipmi.reset_bmc();
            }
        }

        thread::sleep(Duration::from_secs(1));
        reset_lockout = reset_lockout.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rate_history_works() {
        let mut history = super::RateHistory::default();
        assert_eq!(Vec::<u8>::default(), history.rates().collect::<Vec<_>>());
        history.push(0);
        assert_eq!(vec![0], history.rates().collect::<Vec<_>>());
        history.push(1);
        assert_eq!(vec![0, 1], history.rates().collect::<Vec<_>>());
        history.push(2);
        history.push(3);
        assert_eq!(false, history.full());
        history.push(4);
        assert_eq!(true, history.full());
        history.push(5);
        assert_eq!(vec![1, 2, 3, 4, 5], history.rates().collect::<Vec<_>>());
        assert_eq!(true, history.full());
        history.push(6);
        assert_eq!(vec![2, 3, 4, 5, 6], history.rates().collect::<Vec<_>>());
    }
}
