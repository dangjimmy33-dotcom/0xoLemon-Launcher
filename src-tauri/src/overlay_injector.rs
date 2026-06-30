// ============================================================
// 007Launcher — Overlay Injector
// ============================================================
// Detects the architecture of a target process (x86/x64) and
// injects the appropriate overlay DLL using CreateRemoteThread.
// ============================================================

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::mem;

use winapi::shared::minwindef::{BOOL, DWORD, FALSE, HMODULE, LPVOID, MAX_PATH, TRUE};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress};
use winapi::um::memoryapi::{VirtualAllocEx, VirtualFreeEx, WriteProcessMemory};
use winapi::um::processthreadsapi::{CreateRemoteThread, OpenProcess};
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::sysinfoapi::GetSystemInfo;
use winapi::um::winbase::{INFINITE, WAIT_OBJECT_0};
use winapi::um::winnt::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, PROCESS_ALL_ACCESS,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};

#[cfg(target_pointer_width = "64")]
extern "system" {
    fn IsWow64Process(hProcess: winapi::um::winnt::HANDLE, Wow64Process: *mut BOOL) -> BOOL;
}

/// Determine if the target process is 32-bit (running under WoW64) or 64-bit.
fn is_32bit_process(process_handle: winapi::um::winnt::HANDLE) -> Result<bool, String> {
    #[cfg(target_pointer_width = "32")]
    {
        // If we are a 32-bit launcher, we assume the target is 32-bit.
        // A 32-bit process cannot typically query or inject into a 64-bit process anyway.
        Ok(true)
    }
    
    #[cfg(target_pointer_width = "64")]
    {
        let mut is_wow64: BOOL = FALSE;
        if unsafe { IsWow64Process(process_handle, &mut is_wow64) } == 0 {
            return Err(format!("IsWow64Process failed. Error: {}", unsafe { GetLastError() }));
        }
        Ok(is_wow64 == TRUE)
    }
}

/// Injects the overlay DLL into the target process.
/// The `dll_dir` should be the directory containing both `overlay32.dll` and `overlay64.dll`.
pub fn inject(pid: u32, dll_dir: &Path) -> Result<(), String> {
    // 1. Open the target process.
    let h_process = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            FALSE,
            pid,
        )
    };

    if h_process.is_null() {
        return Err(format!("OpenProcess failed for PID {}. Error: {}", pid, unsafe { GetLastError() }));
    }

    // Ensure handle is closed when we return.
    struct ProcessHandle(winapi::um::winnt::HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    let process = ProcessHandle(h_process);

    // 2. Determine architecture.
    let is_32bit = is_32bit_process(process.0)?;
    
    // Choose the right DLL.
    let dll_name = if is_32bit { "overlay32.dll" } else { "overlay64.dll" };
    let dll_path = dll_dir.join(dll_name);
    
    if !dll_path.exists() {
        return Err(format!("DLL not found: {}", dll_path.display()));
    }

    // Convert path to null-terminated UTF-16.
    let mut dll_path_wide: Vec<u16> = OsStr::new(dll_path.as_os_str()).encode_wide().collect();
    dll_path_wide.push(0);
    let path_size = (dll_path_wide.len() * mem::size_of::<u16>()) as usize;

    // 3. Allocate memory in the target process.
    let remote_mem = unsafe {
        VirtualAllocEx(
            process.0,
            ptr::null_mut(),
            path_size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };

    if remote_mem.is_null() {
        return Err(format!("VirtualAllocEx failed. Error: {}", unsafe { GetLastError() }));
    }

    // Cleanup allocated memory if something fails later.
    struct RemoteMemory(winapi::um::winnt::HANDLE, LPVOID);
    impl Drop for RemoteMemory {
        fn drop(&mut self) {
            unsafe { VirtualFreeEx(self.0, self.1, 0, MEM_RELEASE) };
        }
    }
    let _mem_guard = RemoteMemory(process.0, remote_mem);

    // 4. Write the DLL path into the allocated memory.
    let mut bytes_written = 0;
    let write_ok = unsafe {
        WriteProcessMemory(
            process.0,
            remote_mem,
            dll_path_wide.as_ptr() as *const winapi::ctypes::c_void,
            path_size,
            &mut bytes_written,
        )
    };

    if write_ok == 0 || bytes_written != path_size {
        return Err(format!("WriteProcessMemory failed. Error: {}", unsafe { GetLastError() }));
    }

    // 5. Get the address of LoadLibraryW in kernel32.dll.
    // Since kernel32.dll is mapped to the same address in all processes,
    // we can get the address in our own process and use it in the target.
    let kernel32_name: Vec<u16> = OsStr::new("kernel32.dll").encode_wide().chain(std::iter::once(0)).collect();
    let load_library_name = b"LoadLibraryW\0";
    
    let h_kernel32 = unsafe { GetModuleHandleW(kernel32_name.as_ptr()) };
    if h_kernel32.is_null() {
        return Err(format!("GetModuleHandleW(kernel32) failed. Error: {}", unsafe { GetLastError() }));
    }

    let load_library_addr = unsafe { GetProcAddress(h_kernel32, load_library_name.as_ptr() as *const i8) };
    if load_library_addr.is_null() {
        return Err(format!("GetProcAddress(LoadLibraryW) failed. Error: {}", unsafe { GetLastError() }));
    }

    // 6. Create a remote thread to execute LoadLibraryW with the path parameter.
    let h_thread = unsafe {
        CreateRemoteThread(
            process.0,
            ptr::null_mut(),
            0,
            Some(std::mem::transmute(load_library_addr)),
            remote_mem,
            0,
            ptr::null_mut(),
        )
    };

    if h_thread.is_null() {
        return Err(format!("CreateRemoteThread failed. Error: {}", unsafe { GetLastError() }));
    }

    // Wait for the thread to finish.
    unsafe {
        WaitForSingleObject(h_thread, INFINITE);
        CloseHandle(h_thread);
    }

    Ok(())
}
