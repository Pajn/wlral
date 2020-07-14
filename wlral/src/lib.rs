pub(crate) use wayland_sys;
pub(crate) use wlroots_sys::libc;

pub mod macros;

pub mod compositor;
pub mod config;
pub mod event;
pub mod geometry;
pub mod input;
pub mod output;
pub mod output_management_protocol;
pub mod output_manager;
pub mod shell;
pub mod surface;
pub mod wayland_timer;
pub mod window;
pub mod window_management_policy;
pub mod window_manager;

#[cfg(test)]
pub mod test_util {
  use std::ptr;
  use wayland_sys::common::wl_list;
  use wayland_sys::ffi_dispatch;
  use wayland_sys::server::{
    signal::{wl_signal_emit, wl_signal_init},
    wl_signal, WAYLAND_SERVER_HANDLE,
  };

  pub fn new_wl_list() -> wl_list {
    wl_list {
      next: ptr::null_mut(),
      prev: ptr::null_mut(),
    }
  }

  pub fn new_wl_signal() -> wl_signal {
    wl_signal {
      listener_list: new_wl_list(),
    }
  }

  pub struct OwnedWlSignal(*mut wl_signal);
  pub struct WlSignal(*mut wl_signal);

  impl OwnedWlSignal {
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
        ffi_dispatch!(
          WAYLAND_SERVER_HANDLE,
          wl_list_length,
          &(*self.0).listener_list
        )
      }
    }
  }

  impl WlSignal {
    pub fn new() -> OwnedWlSignal {
      let signal = Box::into_raw(Box::new(new_wl_signal()));
      unsafe {
        wl_signal_init(signal);
      }
      OwnedWlSignal(signal)
    }

    pub fn from_ptr(signal: *mut wl_signal) -> WlSignal {
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
        ffi_dispatch!(
          WAYLAND_SERVER_HANDLE,
          wl_list_length,
          &(*self.0).listener_list
        )
      }
    }
  }

  impl Drop for OwnedWlSignal {
    fn drop(&mut self) {
      unsafe {
        Box::from_raw(self.0);
      }
    }
  }
}
