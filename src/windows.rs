// src/windows.rs
//
// Copyright (C) 2023-2024 James Petersen <m@jamespetersen.ca>
// Licensed under Apache 2.0 OR MIT. See LICENSE-APACHE or LICENSE-MIT

use std::{cell::Cell, path::PathBuf, ptr::null_mut};

use widestring::{error::{ContainsNul, Utf16Error}, U16CStr, U16CString, Utf16Str, Utf16String};
use windows::{core::{w, Error as WinError, BSTR, PCWSTR, PWSTR, VARIANT}, Win32::{Foundation::{CloseHandle, LocalFree, CO_E_NOTINITIALIZED, ERROR_INSUFFICIENT_BUFFER, ERROR_NONE_MAPPED, HANDLE, HLOCAL, PSID}, Security::{Authorization::ConvertSidToStringSidW, GetTokenInformation, LookupAccountNameW, TokenUser, SID_NAME_USE, TOKEN_QUERY}, System::{Com::{CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE}, Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE}, Threading::{GetCurrentProcess, OpenProcessToken}, Wmi::{IWbemLocator, WbemLocator, WBEM_FLAG_CONNECT_USE_MAX_WAIT, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE}}, UI::Shell::{FOLDERID_Profile, SHGetKnownFolderPath, KNOWN_FOLDER_FLAG}}};

thread_local! {
    static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

pub type UserIdentifier = String;

#[derive(Debug)]
pub enum GetHomeError {
    WindowsError(WinError),
    Utf16Error(Utf16Error),
    ContainsNul(ContainsNul<u16>),
    NullPointerResult,
}

pub fn get_home<S: AsRef<str>>(username: S) -> Result<Option<PathBuf>, GetHomeError> {
    let Some(s) = get_id(username.as_ref())? else {
        return Ok(None);
    };
    get_home_from_id(&s)
}

pub fn get_home_from_id(id: &UserIdentifier) -> Result<Option<PathBuf>, GetHomeError> {
    const NAMESPACE_PATH: &str = "ROOT\\CIMV2";
    unsafe {
        let instance_fn = || CoCreateInstance::<_, IWbemLocator>(&WbemLocator, None, CLSCTX_INPROC_SERVER);
        let instance = match instance_fn() {
            Ok(v) => v,
            Err(e) => {
                if e != CO_E_NOTINITIALIZED.into() || COM_INITIALIZED.get() {
                    return Err(e.into());
                }
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
                instance_fn()?
            },
        };
        let nms_path_bstr = BSTR::from(NAMESPACE_PATH);
        let svc = instance.ConnectServer(
            &nms_path_bstr,
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            WBEM_FLAG_CONNECT_USE_MAX_WAIT.0,
            &BSTR::new(),
            None,
        )?;
        CoSetProxyBlanket(
            &svc,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )?;

        let query_enum = svc.ExecQuery(&BSTR::from("WQL"), &BSTR::from(format!("SELECT LocalPath FROM Win32_UserProfile WHERE SID = '{id}'")), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)?;
        let mut ret = [None; 1];
        let mut ret_count = 0;
        query_enum.Next(WBEM_INFINITE, &mut ret, &mut ret_count).ok()?;
        if ret_count == 0 {
            return Ok(None);
        }
        let [ret] = ret;
        let ret = ret.ok_or(GetHomeError::NullPointerResult)?;
        let name = w!("LocalPath");
        let mut variant = VARIANT::default();
        let mut vt_type = 0;
        ret.Get(
            name,
            0,
            &mut variant,
            Some(&mut vt_type),
            None
        )?;
        println!("variant thing: {}", variant.as_raw().Anonymous.Anonymous.vt);
    }
    todo!()
}

pub fn get_id<S: AsRef<str>>(username: S) -> Result<Option<UserIdentifier>, GetHomeError> {
    unsafe {
        let username = U16CString::from_str(username)?;
        let mut sid_size = 0;
        let mut domain_size = 0;
        let mut peuse = SID_NAME_USE(0);
        if let Err(e) = LookupAccountNameW(
            None,
            PCWSTR(username.as_ptr()),
            PSID(null_mut()),
            &mut sid_size,
            PWSTR::null(),
            &mut domain_size,
            &mut peuse
        ) {
            if e == ERROR_NONE_MAPPED.into() {
                return Ok(None);
            } else if e != ERROR_INSUFFICIENT_BUFFER.into() {
                return Err(e.into());
            }
        }
        let mut sid_buf = vec![0u8; sid_size as usize];
        let mut domain = vec![0; domain_size as usize];
        LookupAccountNameW(
            None,
            PCWSTR(username.as_ptr()),
            PSID(sid_buf.as_mut_ptr().cast()),
            &mut sid_size,
            PWSTR(domain.as_mut_ptr()),
            &mut domain_size,
            &mut peuse,
        )?;
        sid_to_string(sid_buf).map(Some)
    }
}

pub fn get_my_id() -> Result<UserIdentifier, GetHomeError> {
    unsafe {
        // get the handle of the current process.
        let handle = GetCurrentProcess();
        let mut token_handle = HANDLE(0);
        // get a token to query information about the current process. this handle must be dropped
        // manually with CloseHandle.
        OpenProcessToken(handle, TOKEN_QUERY, &mut token_handle)?;
        // this structure wraps the token handle so it is automatically dropped.
        // this could be written better if try blocks were implemented, and it will probably
        // be modified once they are stabilized for long enough (if ever)
        let token_handle = token_handle;
        let mut buffer_size = 0;
        // get the length of the buffer requried for this query.
        if let Err(e) = GetTokenInformation(token_handle, TokenUser, None, 0, &mut buffer_size) {
            if e != ERROR_INSUFFICIENT_BUFFER.into() {
                let _ = CloseHandle(token_handle);
                return Err(e.into());
            }
        }
        // buffer_size now contains the # of bytes.
        // TODO: check if SID alignment is greater than 1.
        let mut buf = vec![0; buffer_size as usize];
        if let Err(e) = GetTokenInformation(token_handle, TokenUser, Some(buf.as_mut_ptr().cast()), buffer_size, &mut buffer_size) {
            let _ = CloseHandle(token_handle);
            return Err(e.into());
        }
        sid_to_string(buf)
    }
}

pub fn get_my_home() -> Result<Option<PathBuf>, GetHomeError> {
    unsafe {
        let out = SHGetKnownFolderPath(&FOLDERID_Profile, KNOWN_FOLDER_FLAG(0), None)?.0;
        // there isn't any documented case where this will occur, but who knows.
        if out.is_null() {
            return Ok(None);
        }
        let s = U16CStr::from_ptr_str(out).to_os_string().into();
        CoTaskMemFree(Some(out.cast()));
        Ok(Some(s))
    }
}

unsafe fn sid_to_string(mut sid: Vec<u8>) -> Result<UserIdentifier, GetHomeError> {
    let mut str_pointer: PWSTR = PWSTR::null();
    // convert the SID to string.
    ConvertSidToStringSidW(PSID(sid.as_mut_ptr().cast()), &mut str_pointer)?;
    let ret = match U16CStr::from_ptr_str(str_pointer.0).to_string() {
        Ok(v) => v,
        Err(e) => {
            // we already have an error. I won't check for this one.
            LocalFree(HLOCAL(str_pointer.0.cast()));
            return Err(e.into());
        },
    }.to_owned();
    if !LocalFree(HLOCAL(str_pointer.0.cast())).0.is_null() {
        Err(WinError::from_win32())?;
    }
    Ok(ret)
}

impl From<WinError> for GetHomeError {
    fn from(value: WinError) -> Self {
        Self::WindowsError(value)
    }
}

impl From<Utf16Error> for GetHomeError {
    fn from(value: Utf16Error) -> Self {
        Self::Utf16Error(value)
    }
}

impl From<ContainsNul<u16>> for GetHomeError {
    fn from(value: ContainsNul<u16>) -> Self {
        Self::ContainsNul(value)
    }
}
