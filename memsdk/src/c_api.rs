use crate::MemCloudClient;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::Mutex;
use tokio::runtime::Runtime;
use lazy_static::lazy_static;

// Global runtime for C API to execute async tasks
lazy_static! {
    static ref RUNTIME: Runtime = Runtime::new().unwrap();
    static ref CLIENT: Mutex<Option<MemCloudClient>> = Mutex::new(None);
}

#[no_mangle]
pub extern "C" fn memcloud_init() -> c_int {
    RUNTIME.block_on(async {
        match MemCloudClient::connect().await {
            Ok(client) => {
                *CLIENT.lock().unwrap() = Some(client);
                0
            }
            Err(_) => -1,
        }
    })
}

#[no_mangle]
pub extern "C" fn memcloud_store(data: *const c_void, size: usize, out_id: *mut u64) -> c_int {
    if data.is_null() || out_id.is_null() {
        return -1;
    }

    let slice = unsafe { std::slice::from_raw_parts(data as *const u8, size) };
    
    RUNTIME.block_on(async {
        let mut guard = CLIENT.lock().unwrap();
        if let Some(client) = &mut *guard {
            match client.store(slice, crate::Durability::Pinned).await {
                Ok(id) => {
                    unsafe { *out_id = id };
                    0
                }
                Err(_) => -2,
            }
        } else {
            -1 // Not init
        }
    })
}

#[no_mangle]
pub extern "C" fn memcloud_load(id: u64, out_buffer: *mut c_void, buffer_size: usize) -> c_int {
    if out_buffer.is_null() {
        return -1;
    }

    RUNTIME.block_on(async {
        let mut guard = CLIENT.lock().unwrap();
        if let Some(client) = &mut *guard {
            match client.load(id).await {
                Ok(data) => {
                    if data.len() > buffer_size {
                        return -3; // Buffer too small
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr(), out_buffer as *mut u8, data.len());
                    }
                    data.len() as c_int // Return bytes read
                }
                Err(_) => -2, // Not found
            }
        } else {
            -1
        }
    })
}

#[no_mangle]
pub extern "C" fn memcloud_free(id: u64) -> c_int {
    RUNTIME.block_on(async {
        let mut guard = CLIENT.lock().unwrap();
        if let Some(client) = &mut *guard {
            match client.free(id).await {
                Ok(_) => 0,
                Err(_) => -2,
            }
        } else {
            -1
        }
    })
}
