//! Windows access control for the spool: a directory the game can be driven through (`eval` runs
//! anything the game can) must not be usable by other accounts. `%TEMP%` can grant write access to
//! other accounts, and a new directory inherits it, so the directory's rules are replaced with ones
//! for this user and the system alone, and a directory somebody else owns is refused.

use crate::error::{ClientError, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
    OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TokenOwner, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The built-in Administrators group: an elevated process owns what it creates through it, and a
/// directory made by an earlier elevated run is still ours.
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// The calling account and the default owner of what it creates, as SID strings.
struct Identity {
    user: String,
    owner: String,
}

fn failure(what: &str, code: u32) -> ClientError {
    ClientError::msg(format!("{what} (Windows error {code})"))
}

fn last_error(what: &str) -> ClientError {
    failure(
        what,
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32,
    )
}

/// A SID as its string form (`S-1-5-...`).
///
/// # Safety
/// `sid` must point to a valid SID.
unsafe fn sid_to_string(sid: PSID) -> Option<String> {
    let mut text: *mut u16 = null_mut();
    // SAFETY: the caller vouches for `sid`; the call allocates `text`, which is freed below.
    unsafe {
        if ConvertSidToStringSidW(sid, &mut text) == 0 || text.is_null() {
            return None;
        }
        let mut length = 0;
        while *text.add(length) != 0 {
            length += 1;
        }
        let converted = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        LocalFree(text.cast());
        Some(converted)
    }
}

/// The SID that a token information class (`TokenUser`, `TokenOwner`) starts with.
fn token_sid(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<String> {
    // SAFETY: plain Win32 calls; the buffer is sized by the first call and 8-byte aligned, and both
    // `TOKEN_USER` and `TOKEN_OWNER` begin with a pointer to the SID inside the same buffer.
    unsafe {
        let mut needed = 0u32;
        GetTokenInformation(token, class, null_mut(), 0, &mut needed);
        if needed == 0 {
            return Err(last_error("cannot size the token information"));
        }
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        if GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        ) == 0
        {
            return Err(last_error("cannot read the token information"));
        }
        let sid = *(buffer.as_ptr() as *const PSID);
        sid_to_string(sid).ok_or_else(|| last_error("cannot convert the account SID"))
    }
}

fn identity() -> Result<Identity> {
    // SAFETY: plain Win32 calls on this process's own token, which is closed again.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(last_error("cannot open the process token"));
        }
        let user = token_sid(token, TokenUser);
        let owner = token_sid(token, TokenOwner);
        CloseHandle(token);
        Ok(Identity {
            user: user?,
            owner: owner?,
        })
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// The owner of the directory, as a SID string.
fn owner_of(path: &Path) -> Result<String> {
    let name = wide(path);
    // SAFETY: plain Win32 calls; the descriptor the call allocates is freed after the owner SID,
    // which lives inside it, has been copied out as a string.
    unsafe {
        let mut owner: PSID = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let status = GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        );
        if status != ERROR_SUCCESS {
            return Err(failure("cannot read the owner of the directory", status));
        }
        let text = sid_to_string(owner);
        LocalFree(descriptor);
        text.ok_or_else(|| last_error("cannot convert the owner SID"))
    }
}

/// Replaces the directory's access rules with full control for this user and the system, inherited
/// by everything inside, and cuts off inheritance from the parent.
fn lock_down(path: &Path, user: &str) -> Result<()> {
    let rules: Vec<u16> = format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{user})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let name = wide(path);
    // SAFETY: plain Win32 calls; the descriptor holds the DACL the last call reads, and is freed
    // after it.
    unsafe {
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            rules.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return Err(last_error("cannot build the access rules"));
        }
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl: *mut ACL = null_mut();
        let found = GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted);
        let status = if found != 0 && present != 0 {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                dacl,
                null(),
            )
        } else {
            1
        };
        LocalFree(descriptor);
        if status != ERROR_SUCCESS {
            return Err(failure(
                &format!("cannot restrict {} to this user", path.display()),
                status,
            ));
        }
    }
    Ok(())
}

/// Makes `path` (an existing directory that is not a link) usable by this user and the system alone.
/// A directory owned by another account is refused: that account could grant itself access again.
pub fn make_private(path: &Path) -> Result<()> {
    let who = identity()?;
    let owner = owner_of(path)?;
    if owner != who.user && owner != who.owner && owner != ADMINISTRATORS && owner != "S-1-5-18" {
        return Err(ClientError::msg(format!(
            "{} belongs to another account ({owner}); refusing to use it. Set DEV_BRIDGE_ROOT to a directory of your own",
            path.display()
        )));
    }
    lock_down(path, &who.user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::{
        ACL_SIZE_INFORMATION, AclSizeInformation, GetAclInformation, GetSecurityDescriptorControl,
        SE_DACL_PROTECTED,
    };

    /// Whether the directory's rules are protected from inheritance, and how many there are.
    fn rules_of(path: &Path) -> (bool, u32) {
        let name = wide(path);
        // SAFETY: plain Win32 calls; the descriptor is freed after the values are read.
        unsafe {
            let mut dacl: *mut ACL = null_mut();
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            let status = GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            );
            assert_eq!(status, ERROR_SUCCESS);
            let mut control = 0u16;
            let mut revision = 0u32;
            assert_ne!(
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision),
                0
            );
            let mut size = std::mem::zeroed::<ACL_SIZE_INFORMATION>();
            assert_ne!(
                GetAclInformation(
                    dacl,
                    (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ),
                0
            );
            LocalFree(descriptor);
            (control & SE_DACL_PROTECTED != 0, size.AceCount)
        }
    }

    #[test]
    fn a_directory_is_left_with_two_protected_rules_the_user_and_the_system() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("private");
        std::fs::create_dir(&target).unwrap();
        make_private(&target).unwrap();
        assert_eq!(rules_of(&target), (true, 2));
        std::fs::write(target.join("inside.txt"), "x").unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("inside.txt")).unwrap(),
            "x"
        );
        make_private(&target).unwrap();
        assert_eq!(
            rules_of(&target),
            (true, 2),
            "doing it twice changes nothing"
        );
    }
}
