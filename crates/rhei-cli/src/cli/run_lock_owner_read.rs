/// Read run-lock ownership without disturbing the held byte-range lock.
/// Windows denies ordinary reads through another handle but permits mapped views.
/// §REQ-cross-platform §FS-rhei-summary.5
#[cfg(not(windows))]
fn read_run_lock_owner(path: &Path) -> std::io::Result<String> {
    fs::read_to_string(path)
}

#[cfg(windows)]
fn read_run_lock_owner(path: &Path) -> std::io::Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ,
        MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READONLY,
    };

    struct Mapping(HANDLE);
    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the live handle returned by CreateFileMappingW.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    struct View(MEMORY_MAPPED_VIEW_ADDRESS);
    impl Drop for View {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the live view returned by MapViewOfFile.
            unsafe {
                UnmapViewOfFile(self.0);
            }
        }
    }

    let file = fs::File::open(path)?;
    let len = usize::try_from(file.metadata()?.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "run-lock ownership is too large to inspect",
        )
    })?;
    if len == 0 {
        return Ok(String::new());
    }

    // SAFETY: the file handle stays live through creation, null security/name
    // pointers request documented defaults, and the mapping is read-only.
    let mapping = unsafe {
        CreateFileMappingW(
            file.as_raw_handle(),
            std::ptr::null(),
            PAGE_READONLY,
            0,
            0,
            std::ptr::null(),
        )
    };
    if mapping.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let mapping = Mapping(mapping);

    // SAFETY: `mapping` remains live, offsets are zero, and a successful call
    // guarantees that the requested `len` bytes belong to the returned view.
    let address = unsafe { MapViewOfFile(mapping.0, FILE_MAP_READ, 0, 0, len) };
    if address.Value.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let view = View(address);

    // SAFETY: `view` remains mapped for `len` readable bytes while this slice
    // is copied into the returned owned string.
    let bytes = unsafe { std::slice::from_raw_parts(view.0.Value.cast::<u8>(), len) };
    String::from_utf8(bytes.to_vec())
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
}
