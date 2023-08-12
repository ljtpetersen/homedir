// src/windows.rs
//
// Copyright (C) 2023 James Petersen <m@jamespetersen.ca>
// Licensed under Apache 2.0 OR MIT. See LICENSE-APACHE or LICENSE-MIT

use std::alloc::alloc;
use std::alloc::dealloc;
use std::alloc::Layout;
use std::error::Error;
use std::ffi::c_void;
use std::fmt;
use std::path::PathBuf;
use std::ptr::null;
use std::ptr::null_mut;

use widestring::error::ContainsNul;
use widestring::u16cstr;
use widestring::u16str;
use widestring::U16CStr;
use widestring::U16CString;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Foundation::ERROR_NONE_MAPPED;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::WIN32_ERROR;
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::GetTokenInformation;
use windows_sys::Win32::Security::LookupAccountNameW;
use windows_sys::Win32::Security::TokenUser;
use windows_sys::Win32::Security::SID_NAME_USE;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::Security::TOKEN_USER;
use windows_sys::Win32::System::Memory::LocalFree;
use windows_sys::Win32::System::Registry::RegGetValueW;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::Registry::RRF_RT_REG_SZ;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::Threading::OpenProcessToken;

/// The error type returned by [`get_home`] upon failure.
#[derive(Debug)]
pub enum GetHomeError {
    /// Represents an error code returned by the Windows API.
    /// An interpretation of this error code's meaning
    /// can be found on
    /// [Microsoft's documentation page](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-).
    WindowsError(WIN32_ERROR),
    /// This variant occurs when the user passes a string reference which contains a null character `'\0'` to the
    /// [`get_home`] function.
    NulError(ContainsNul<u16>),
}

/// Get a user's home directory path.
///
/// If some error occurs when obtaining the path, `Err` is returned. If `username`
/// contains a null character (`'\0'`), an error is guaranteed to be returned.
/// If no user associated with `username` could be found, `Ok(None)` is returned.
/// Otherwise, the path of the user's home directory is returned.
///
/// # Example
/// ```no_run
/// use homedir::get_home;
///
/// // This assumes there is a user named `Administrator` which has
/// // `C:\Users\Administrator` as a home directory.
/// assert_eq!(
///     "C:\\Users\\Administrator".as_ref(),
///     get_home("Administrator").unwrap().unwrap().as_path()
/// );
/// assert!(get_home("NonExistentUser").unwrap().is_none());
/// assert!(get_home("User\0Name").is_err());
/// ```
///
/// This function obtains the home directory path in the following manner. First,
/// obtain the [SID](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/understand-security-identifiers)
/// associated with the username using the
/// [`LookupAccountNameW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-lookupaccountnamew) function.
/// Then, convert this SID to a string, and read the registry key
/// `\HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\<SID Here>\ProfileImagePath`. This should
/// be the home directory path.
pub fn get_home<S: AsRef<str>>(username: S) -> Result<Option<PathBuf>, GetHomeError> {
    unsafe {
        // get the SID and the memory layout to later deallocate it.
        let Some((buf, lay)) = get_user_sid(username.as_ref())? else {
            // if the user could not be associated with an SID, return none.
            return Ok(None);
        };
        // Get the home directory associated with the sid from the registry.
        let ret = get_sid_home(buf);
        // deallocate the sid buf.
        dealloc(buf.cast(), lay);
        // return.
        Ok(Some(ret?))
    }
}

/// Get this process' user's home directory.
///
/// If some error occurs when obtaining the path, `Err` is returned.
/// On Windows, this function will never return `Ok(None)`.
///
/// # Example
/// ```no_run
/// use homedir::get_my_home;
///
/// // This assumes that the process' user has "/home/jpetersen" as home directory.
/// assert_eq!(
///     "C:\\Users\\m".as_ref(),
///     get_my_home().unwrap().unwrap().as_path()
/// );
/// ```
///
/// This function operates in a very similar manner to [`get_home`]. However,
/// instead of getting the user's SID through [`LookupAccountNameW`],
/// it obtains it using
/// [`GetTokenInformation`](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation).
pub fn get_my_home() -> Result<Option<PathBuf>, GetHomeError> {
    Ok(unsafe {
        // get the SID and the memory layout to later deallocate it.
        // buf is a pointer to a TOKEN_USER structure.
        let (buf, lay) = get_my_sid()?;
        // get the home directory associated with the sid from the registry.
        let tmp = get_sid_home((*buf.cast::<TOKEN_USER>()).User.Sid.cast())?;
        // deallocate the structore
        dealloc(buf.cast(), lay);
        // return.
        Some(tmp)
    })
}

unsafe fn get_my_sid() -> Result<(*mut c_void, Layout), GetHomeError> {
    // get the handle of the current process
    let handle = GetCurrentProcess();
    let mut buffer_size = 0u32;
    let mut token_handle = 0isize;
    // get a token to query information about the current process.
    let mut check_err = OpenProcessToken(handle, TOKEN_QUERY, &mut token_handle);
    if check_err == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    // query the length of the buffer needed to get store the TOKEN_USER structure.
    check_err = GetTokenInformation(token_handle, TokenUser, null_mut(), 0, &mut buffer_size);
    if check_err == 0 {
        let err = GetLastError();
        if err != ERROR_INSUFFICIENT_BUFFER {
            CloseHandle(token_handle);
            return Err(GetHomeError::WindowsError(err));
        }
    }
    // buffer_size now contains # of bytes.
    // don't use vec because we need to ensure proper alignment.
    let layout = Layout::from_size_align(buffer_size as usize, 16).unwrap();
    // allocate buffer
    let ptr = alloc(layout).cast();
    // get the TOKEN_USER structure into ptr.
    check_err = GetTokenInformation(token_handle, TokenUser, ptr, buffer_size, &mut buffer_size);
    if check_err == 0 {
        dealloc(ptr.cast(), layout);
        CloseHandle(token_handle);
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    // make sure to close the handle.
    if CloseHandle(token_handle) == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    Ok((ptr, layout))
}

unsafe fn get_user_sid(username: &str) -> Result<Option<(*mut c_void, Layout)>, GetHomeError> {
    let mut sid_size = 0u32;
    let mut domain_size = 0u32;
    let mut peuse: SID_NAME_USE = 0;
    let username = U16CString::from_str(username)?;
    // get the length of the buffers needed to store the sid and domain.
    let mut check_err = LookupAccountNameW(
        null(),
        username.as_ptr(),
        null_mut(),
        &mut sid_size,
        null_mut(),
        &mut domain_size,
        &mut peuse,
    );
    if check_err == 0 {
        match GetLastError() {
            ERROR_INSUFFICIENT_BUFFER => {}
            // this is returned if the user could not be associated with an SID
            ERROR_NONE_MAPPED => return Ok(None),
            e => return Err(GetHomeError::WindowsError(e)),
        }
    }
    let layout = Layout::from_size_align(sid_size as usize, 16).unwrap();
    let domain_layout = Layout::array::<u16>(domain_size as usize).unwrap();
    let sid = alloc(layout).cast();
    // the domain field is not used after the call. However, it mut be allocated
    // for the call to get the SID.
    let domain = alloc(domain_layout).cast();
    // Lookup the SID.
    check_err = LookupAccountNameW(
        null(),
        username.as_ptr(),
        sid,
        &mut sid_size,
        domain,
        &mut domain_size,
        &mut peuse,
    );
    // immediately free the domain field after the call.
    dealloc(domain.cast(), domain_layout);
    if check_err == 0 {
        dealloc(sid.cast(), layout);
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    Ok(Some((sid, layout)))
}

unsafe fn get_sid_home(sid: *mut c_void) -> Result<PathBuf, GetHomeError> {
    let mut str_pointer: *mut u16 = null_mut();
    // convert the SID to a string.
    let check_err = ConvertSidToStringSidW(sid, &mut str_pointer);
    if check_err == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    // Prepend the SID string with the path in the registry. The HKLM value is not added to the prefix,
    // as it is the root key used to make the request.
    let mut path =
        u16str!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList\\").to_owned();
    path.push(U16CStr::from_ptr_str(str_pointer).as_ustr());
    // Free the memory allocated for the SID string.
    if LocalFree(str_pointer as isize) != 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    let path = U16CString::from_vec(path.into_vec())?;
    let mut out_path_length = 0u32;
    let key = HKEY_LOCAL_MACHINE;
    let name = u16cstr!("ProfileImagePath").as_ptr();
    // check the length required to store the home directory.
    let mut check_err = RegGetValueW(
        key,
        path.as_ptr(),
        name,
        RRF_RT_REG_SZ,
        null_mut(),
        null_mut(),
        &mut out_path_length,
    );
    if check_err != ERROR_SUCCESS {
        return Err(GetHomeError::WindowsError(check_err));
    }
    let layout = Layout::from_size_align(out_path_length as usize, 2).unwrap();
    // allocate buffer for the home directory
    let out_path = alloc(layout).cast();
    // get the value from the registry.
    check_err = RegGetValueW(
        key,
        path.as_ptr(),
        name,
        RRF_RT_REG_SZ,
        null_mut(),
        out_path,
        &mut out_path_length,
    );
    if check_err != ERROR_SUCCESS {
        dealloc(out_path.cast(), layout);
        return Err(GetHomeError::WindowsError(check_err));
    }
    let ret = U16CStr::from_ptr_str(out_path.cast()).to_os_string().into();
    // deallocate the temporary buffer used to store the registry value.
    dealloc(out_path.cast(), layout);
    Ok(ret)
}

impl From<ContainsNul<u16>> for GetHomeError {
    fn from(value: ContainsNul<u16>) -> Self {
        Self::NulError(value)
    }
}

impl fmt::Display for GetHomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NulError(e) => write!(f, "null error: {e}"),
            Self::WindowsError(e) => write!(f, "windows error: code = {e}"),
        }
    }
}

impl Error for GetHomeError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            Self::NulError(e) => Some(e),
            Self::WindowsError(_) => None,
        }
    }
}
