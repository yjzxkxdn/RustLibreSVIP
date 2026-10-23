//! Small platform helpers used for diagnostics.

/// `` `DOMAIN\user`, integrity level `` — enough to tell an ordinary launch
/// apart from a sandboxed / restricted one when a write is refused.
pub fn process_context() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let who = if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    };
    format!("{who}，完整性级别 {}", integrity_level())
}

/// True when the process runs with a low/untrusted integrity level: such a
/// process cannot write to the user's normal folders (no-write-up), which is
/// what security sandboxes do to unknown executables.
pub fn is_restricted() -> bool {
    matches!(integrity_level().as_str(), "Low" | "Untrusted")
}

#[cfg(windows)]
fn integrity_level() -> String {
    use std::ffi::c_void;

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(
            process: *mut c_void,
            desired_access: u32,
            token: *mut *mut c_void,
        ) -> i32;
        fn GetTokenInformation(
            token: *mut c_void,
            class: u32,
            info: *mut c_void,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_INTEGRITY_LEVEL: u32 = 25;

    unsafe {
        let mut token: *mut c_void = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return "未知".to_owned();
        }
        let mut buffer = [0u8; 128];
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TOKEN_INTEGRITY_LEVEL,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
            &mut len,
        );
        CloseHandle(token);
        if ok == 0 {
            return "未知".to_owned();
        }
        // TOKEN_MANDATORY_LABEL { SID_AND_ATTRIBUTES { PSID; DWORD } }
        let sid = *(buffer.as_ptr() as *const *const u8);
        if sid.is_null() {
            return "未知".to_owned();
        }
        let count = *sid.add(1) as usize;
        if count == 0 {
            return "未知".to_owned();
        }
        let last = std::ptr::read_unaligned(sid.add(8 + (count - 1) * 4) as *const u32);
        match last {
            0x0000 => "Untrusted".to_owned(),
            0x1000 => "Low".to_owned(),
            0x2000 => "Medium".to_owned(),
            0x2100 => "Medium Plus".to_owned(),
            0x3000 => "High".to_owned(),
            0x4000 => "System".to_owned(),
            other => format!("0x{other:X}"),
        }
    }
}

#[cfg(not(windows))]
fn integrity_level() -> String {
    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_context_is_populated() {
        let context = super::process_context();
        assert!(context.contains('，'), "unexpected context: {context}");
        assert!(!context.contains("未知"), "integrity level unknown: {context}");
    }
}