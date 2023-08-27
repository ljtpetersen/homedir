// src/windows.rs
//
// Copyright (C) 2023 James Petersen <m@jamespetersen.ca>
// Licensed under Apache 2.0 OR MIT. See LICENSE-APACHE or LICENSE-MIT

use std::alloc::alloc;
use std::alloc::dealloc;
use std::alloc::Layout;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::c_void;
use std::fmt;
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::ptr::null;
use std::ptr::null_mut;

use widestring::error::ContainsNul;
use widestring::U16CStr;
use widestring::U16CString;

use serde::Deserialize;

use windows_sys::core::HRESULT;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Foundation::ERROR_NONE_MAPPED;
use windows_sys::Win32::Foundation::E_INVALIDARG;
use windows_sys::Win32::Foundation::S_OK;
use windows_sys::Win32::Foundation::WIN32_ERROR;
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::GetTokenInformation;
use windows_sys::Win32::Security::LookupAccountNameW;
use windows_sys::Win32::Security::TokenUser;
use windows_sys::Win32::Security::SID_NAME_USE;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::Security::TOKEN_USER;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Memory::LocalFree;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::Threading::OpenProcessToken;
use windows_sys::Win32::UI::Shell::FOLDERID_Profile;
use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;

use wmi::COMLibrary;
use wmi::WMIConnection;
use wmi::WMIError;

/// An identifier for a user.
///
/// This is the text representation of a user's SID.
pub type UserIdentifier = String;

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
    /// Represents an HRESULT returned by the Windows API.
    HResult(HRESULT),
    /// Represents an error returned by the wmi crate.
    WMIError(WMIError),
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_UserProfile")]
#[serde(rename_all = "PascalCase")]
struct UserProfile {
    local_path: String,
}

/// Get a user's home directory path.
///
/// If some error occurs when obtaining the path, `Err` is returned. If no user
/// associated with `username` could be found, `Ok(None)` is returned. Otherwise,
/// the path to the user's home directory is returned.
///
/// This function first gets the user's id from [`get_id`](get_id).
/// Then, it passes this SID to [`get_home_from_id`](get_home_from_id).
///
/// # Example
/// ```no_run
/// use homedir::get_home;
///
/// // This assumes there is a user on the local machine named "Administrator"
/// // whose profile path is "C:\Users\Administrator".
/// assert_eq!(
///     std::path::Path::new("C:\\Users\\Administrator"),
///     get_home("Administrator").unwrap().unwrap().as_path(),
/// );
/// ```
pub fn get_home<S: AsRef<str>>(username: S) -> Result<Option<PathBuf>, GetHomeError> {
    let Some(s) = get_id(username.as_ref())? else {
        return Ok(None);
    };
    get_home_from_id(&s)
}

/// Get a user's home directory path from their ID.
///
/// The passed SID is used in a WMI query to get the
/// [`Win32_UserProfile`](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/legacy/ee886409(v=vs.85))
/// class' LocalPath field. Note that WMI queries require
/// [`CoInitializeEx`](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex)
/// to be called first. This function uses the `COINIT_MULTITHREADED` flag when calling this function, which
/// could cause an issue if the initialization function is called with a different flag.
///
/// # Example
/// ```no_run
/// use homedir::get_home_from_id;
///
/// // This assumes that the current user's profile path is
/// // "C:\Users\jpetersen".
/// assert_eq!(
///     std::path::Path::new("C:\\Users\\jpetersen"),
///     get_home_from_id(get_my_id().unwrap()).unwrap().unwrap(),
/// );
/// ```
pub fn get_home_from_id(id: &UserIdentifier) -> Result<Option<PathBuf>, GetHomeError> {
    thread_local! {
        static COM_LIB: COMLibrary = COMLibrary::new().unwrap();
    }
    let con = WMIConnection::new(COM_LIB.with(|f| *f))?;
    let mut filters = HashMap::with_capacity(1);
    filters.insert("SID".to_owned(), wmi::FilterValue::String(id.clone()));
    let v = con.filtered_query::<UserProfile>(&filters)?;
    let Some(p) = v.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(p.local_path.into()))
}

/// Get a user's id from their uesrname.
///
/// This function uses
/// [`LookupAccountNameW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-lookupaccountnamew)
/// to get a user's SID, before converting it to a string using
/// [`ConvertSidToStringSidW`](https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertsidtostringsidw).
pub fn get_id<S: AsRef<str>>(username: S) -> Result<Option<UserIdentifier>, GetHomeError> {
    unsafe {
        let Some((sid, lay)) = get_user_sid(username.as_ref())? else {
            return Ok(None);
        };
        let ret = sid_to_string(sid);
        dealloc(sid.cast(), lay);
        ret.map(Option::Some)
    }
}

/// Get the home directory of the process' user.
///
/// This function uses the
/// [`SHGetKnownFolderPath`](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath)
/// function to get the profile directory path of the current user.
///
/// # Example
/// ```no_run
/// use homedir::get_my_home;
///
/// // This assumes that the current process' user's profile directory path is
/// // "C:\Users\jpetersen".
/// assert_eq!(
///     std::path::Path::new("C:\\Users\\jpetersen"),
///     get_my_home().unwrap().unwrap(),
/// );
/// ```
pub fn get_my_home() -> Result<Option<PathBuf>, GetHomeError> {
    unsafe {
        let mut out = MaybeUninit::uninit();
        let hres = SHGetKnownFolderPath(&FOLDERID_Profile, 0, 0, out.as_mut_ptr());
        let out = out.assume_init();
        match hres {
            S_OK => {}
            E_INVALIDARG => return Ok(None),
            _ => {
                CoTaskMemFree(out.cast());
                return Err(GetHomeError::HResult(hres));
            }
        }
        let s = U16CStr::from_ptr_str(out).to_os_string().into();
        CoTaskMemFree(out.cast());
        Ok(Some(s))
    }
}

/// Get the user ID of the current process' user.
///
/// This function uses the
/// [`GetTokenInformation`](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
/// with a handle obtained from the
/// [`OpenProcessToken`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocesstoken).
pub fn get_my_id() -> Result<UserIdentifier, GetHomeError> {
    unsafe {
        // Get this process' user's SID
        let (sid, lay) = get_my_sid()?;
        // Convert it to a string
        let ret = sid_to_string((*sid.cast::<TOKEN_USER>()).User.Sid.cast());
        // Free the SID
        dealloc(sid.cast(), lay);
        // Return
        ret
    }
}

unsafe fn get_process_token() -> Result<isize, GetHomeError> {
    // get the handle of the current process
    let handle = GetCurrentProcess();
    let mut token_handle = 0isize;
    // get a token to query information about the current process.
    if OpenProcessToken(handle, TOKEN_QUERY, &mut token_handle) == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    Ok(token_handle)
}

unsafe fn get_my_sid() -> Result<(*mut c_void, Layout), GetHomeError> {
    let token_handle = get_process_token()?;
    let mut buffer_size = 0;
    // query the length of the buffer needed to get store the TOKEN_USER structure.
    if GetTokenInformation(token_handle, TokenUser, null_mut(), 0, &mut buffer_size) == 0 {
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
    if GetTokenInformation(token_handle, TokenUser, ptr, buffer_size, &mut buffer_size) == 0 {
        dealloc(ptr.cast(), layout);
        let err = GetLastError();
        CloseHandle(token_handle);
        return Err(GetHomeError::WindowsError(err));
    }
    // make sure to close the handle.
    if CloseHandle(token_handle) == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    Ok((ptr, layout))
}

unsafe fn sid_to_string(sid: *mut c_void) -> Result<UserIdentifier, GetHomeError> {
    let mut str_pointer: *mut u16 = null_mut();
    // convert the SID to a string.
    if ConvertSidToStringSidW(sid, &mut str_pointer) == 0 {
        return Err(GetHomeError::WindowsError(GetLastError()));
    }
    let ret = U16CStr::from_ptr_str(str_pointer).to_string().unwrap();
    if LocalFree(str_pointer as isize) != 0 {
        Err(GetHomeError::WindowsError(GetLastError()))
    } else {
        Ok(ret)
    }
}

unsafe fn get_user_sid(username: &str) -> Result<Option<(*mut c_void, Layout)>, GetHomeError> {
    let mut sid_size = 0u32;
    let mut domain_size = 0u32;
    let mut peuse: SID_NAME_USE = 0;
    let username = U16CString::from_str(username)?;
    // get the length of the buffers needed to store the sid and domain.
    if LookupAccountNameW(
        null(),
        username.as_ptr(),
        null_mut(),
        &mut sid_size,
        null_mut(),
        &mut domain_size,
        &mut peuse,
    ) == 0
    {
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
    let check_err = LookupAccountNameW(
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

impl From<ContainsNul<u16>> for GetHomeError {
    fn from(value: ContainsNul<u16>) -> Self {
        Self::NulError(value)
    }
}

impl From<WMIError> for GetHomeError {
    fn from(value: WMIError) -> Self {
        Self::WMIError(value)
    }
}

impl fmt::Display for GetHomeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NulError(e) => write!(f, "null error: {e}"),
            Self::WindowsError(e) => write!(f, "windows error: code = {e:#x}"),
            Self::HResult(e) => write!(f, "windows error: hresult = {e:#x}"),
            Self::WMIError(e) => write!(f, "wmi error: {e}"),
        }
    }
}

impl Error for GetHomeError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            Self::NulError(e) => Some(e),
            Self::WMIError(e) => Some(e),
            Self::WindowsError(_) | Self::HResult(_) => None,
        }
    }
}
