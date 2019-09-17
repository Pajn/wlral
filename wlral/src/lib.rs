pub(crate) use wayland_sys;
pub(crate) use wlroots_sys::libc;

pub mod macros;

pub mod compositor;
pub mod geometry;
pub mod input;
pub mod output;
pub mod shell;
pub mod surface;

#[cfg(test)]
pub mod test_util {
  use std::ptr;
  use wayland_sys::ffi_dispatch;
  use wayland_sys::common::{wl_list};
  use wayland_sys::server::{WAYLAND_SERVER_HANDLE, wl_signal, signal::{wl_signal_emit, wl_signal_init}};

  pub struct WlSignal(*mut wl_signal);

  impl WlSignal {
    pub fn new() -> WlSignal {
      let signal = Box::into_raw(Box::new(wl_signal {
        listener_list: wl_list {
          next: ptr::null_mut(),
          prev: ptr::null_mut(),
        }
      }));
      unsafe {
        wl_signal_init(signal);
      }
      WlSignal(signal)
    }

    pub fn ptr(&self) -> *mut wl_signal {
      self.0
    }

    pub fn emit(&self) {
      unsafe {
        wl_signal_emit(self.0, ptr::null_mut());
      }
    }

    pub fn listener_count(&self) -> i32 {
      unsafe { 
        ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_list_length, &(*self.0).listener_list)
      }
    }
  }

  impl Drop for WlSignal {
    fn drop(&mut self) {
      unsafe {
        Box::from_raw(self.0);
      }
    }
  }
}
