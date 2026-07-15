use super::error::{Result, SmcError};
use super::value::SmcKey;

const SMC_SELECTOR: u32 = 2;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_READ_INDEX: u8 = 8;
const SMC_CMD_READ_KEY_INFO: u8 = 9;
const SMC_KEY_NOT_FOUND: u8 = 0x84;
const MAX_SMC_DATA_SIZE: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct SmcKeyDataVersion {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct SmcPLimitData {
    version: u16,
    length: u16,
    cpu_p_limit: u32,
    gpu_p_limit: u32,
    mem_p_limit: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SmcKeyInfo {
    pub data_size: u32,
    pub data_type: u32,
    pub data_attributes: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct SmcKeyData {
    key: u32,
    version: SmcKeyDataVersion,
    p_limit_data: SmcPLimitData,
    key_info: SmcKeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; MAX_SMC_DATA_SIZE],
}

#[derive(Debug)]
pub(super) struct SmcConnection {
    handle: u32,
}

impl SmcConnection {
    #[cfg(target_os = "macos")]
    pub fn open() -> Result<Self> {
        use std::ffi::{c_char, c_void};

        type IoObject = u32;
        type IoConnect = u32;

        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            static mach_task_self_: u32;
            fn IOServiceMatching(name: *const c_char) -> *mut c_void;
            fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> IoObject;
            fn IOServiceOpen(
                service: IoObject,
                owning_task: u32,
                connection_type: u32,
                connect: *mut IoConnect,
            ) -> i32;
            fn IOObjectRelease(object: IoObject) -> i32;
        }

        let matching = unsafe { IOServiceMatching(c"AppleSMC".as_ptr()) };
        if matching.is_null() {
            return Err(SmcError::ServiceMatching);
        }
        let service = unsafe { IOServiceGetMatchingService(0, matching) };
        if service == 0 {
            return Err(SmcError::ServiceNotFound);
        }

        let mut handle = 0;
        let status = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut handle) };
        let _ = unsafe { IOObjectRelease(service) };
        if status != 0 {
            return Err(SmcError::Open { status });
        }
        if handle == 0 {
            return Err(SmcError::NullConnection);
        }
        Ok(Self { handle })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open() -> Result<Self> {
        Err(SmcError::UnsupportedPlatform)
    }

    pub fn key_count(&self) -> Result<u32> {
        let key = SmcKey::from_bytes(*b"#KEY");
        let info = self.read_key_info(key)?;
        let bytes = self.read_bytes(key, info)?;
        if info.data_size != 4 {
            return Err(SmcError::KeyCountSize {
                actual: info.data_size,
            });
        }
        Ok(u32::from_be_bytes(
            bytes[..4].try_into().expect("four bytes"),
        ))
    }

    pub fn key_by_index(&self, index: u32) -> Result<SmcKey> {
        let input = SmcKeyData {
            data8: SMC_CMD_READ_INDEX,
            data32: index,
            ..SmcKeyData::default()
        };
        Ok(SmcKey::from_raw(self.call(&input)?.key))
    }

    pub fn read_key_info(&self, key: SmcKey) -> Result<SmcKeyInfo> {
        let input = SmcKeyData {
            key: key.raw(),
            data8: SMC_CMD_READ_KEY_INFO,
            ..SmcKeyData::default()
        };
        let output = self.call(&input)?;
        if output.key_info.data_size as usize > MAX_SMC_DATA_SIZE {
            return Err(SmcError::KeyDataSize {
                key: key.name(),
                actual: output.key_info.data_size,
                maximum: MAX_SMC_DATA_SIZE,
            });
        }
        Ok(output.key_info)
    }

    pub fn read_bytes(&self, key: SmcKey, info: SmcKeyInfo) -> Result<[u8; 32]> {
        let input = SmcKeyData {
            key: key.raw(),
            key_info: info,
            data8: SMC_CMD_READ_BYTES,
            ..SmcKeyData::default()
        };
        Ok(self.call(&input)?.bytes)
    }

    #[cfg(target_os = "macos")]
    fn call(&self, input: &SmcKeyData) -> Result<SmcKeyData> {
        use std::ffi::c_void;
        use std::mem::size_of;

        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOConnectCallStructMethod(
                connection: u32,
                selector: u32,
                input: *const c_void,
                input_size: usize,
                output: *mut c_void,
                output_size: *mut usize,
            ) -> i32;
        }

        let mut output = SmcKeyData::default();
        let mut output_size = size_of::<SmcKeyData>();
        let status = unsafe {
            IOConnectCallStructMethod(
                self.handle,
                SMC_SELECTOR,
                std::ptr::from_ref(input).cast::<c_void>(),
                size_of::<SmcKeyData>(),
                std::ptr::from_mut(&mut output).cast::<c_void>(),
                &mut output_size,
            )
        };
        if status != 0 {
            return Err(SmcError::Call { status });
        }
        if output_size != size_of::<SmcKeyData>() {
            return Err(SmcError::OutputSize {
                actual: output_size,
                expected: size_of::<SmcKeyData>(),
            });
        }
        if output.result == SMC_KEY_NOT_FOUND {
            return Err(SmcError::KeyNotFound {
                key: SmcKey::from_raw(input.key).name(),
            });
        }
        if output.result != 0 {
            return Err(SmcError::SmcResult {
                key: SmcKey::from_raw(input.key).name(),
                result: output.result,
            });
        }
        Ok(output)
    }

    #[cfg(not(target_os = "macos"))]
    fn call(&self, _input: &SmcKeyData) -> Result<SmcKeyData> {
        Err(SmcError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "macos")]
impl Drop for SmcConnection {
    fn drop(&mut self) {
        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOServiceClose(connection: u32) -> i32;
        }
        let _ = unsafe { IOServiceClose(self.handle) };
    }
}

#[cfg(target_os = "macos")]
pub(super) fn cpu_brand() -> Result<String> {
    use std::ffi::{c_char, c_void};

    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old_value: *mut c_void,
            old_len: *mut usize,
            new_value: *mut c_void,
            new_len: usize,
        ) -> i32;
    }

    let mut length = 0;
    let status = unsafe {
        sysctlbyname(
            c"machdep.cpu.brand_string".as_ptr(),
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(SmcError::CpuBrandSysctl {
            operation: "query CPU brand length",
            status,
        });
    }
    if length <= 1 {
        return Err(SmcError::EmptyCpuBrand);
    }

    let mut bytes = vec![0_u8; length];
    let status = unsafe {
        sysctlbyname(
            c"machdep.cpu.brand_string".as_ptr(),
            bytes.as_mut_ptr().cast::<c_void>(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return Err(SmcError::CpuBrandSysctl {
            operation: "read CPU brand",
            status,
        });
    }
    bytes.truncate(length);
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(SmcError::CpuBrandUtf8)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn cpu_brand() -> Result<String> {
    Err(SmcError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{SmcKeyData, SmcKeyInfo};

    #[test]
    fn protocol_layout_matches_apple_smc_user_client() {
        assert_eq!(size_of::<SmcKeyInfo>(), 12);
        assert_eq!(size_of::<SmcKeyData>(), 80);
    }
}
