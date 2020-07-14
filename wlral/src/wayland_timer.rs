use log::error;
use std::{ffi::c_void, panic};
use wayland_sys::{
  ffi_dispatch,
  server::{wl_display, wl_event_source},
};
use wlroots_sys::WAYLAND_SERVER_HANDLE;

type Callback = extern "C" fn(*mut c_void) -> i32;

/// Unpack a Rust closure, extracting a `void*` pointer to the data and a
/// trampoline function which can be used to invoke it.
///
/// # Safety
///
/// It is the user's responsibility to ensure the closure outlives the returned
/// `void*` pointer.
///
/// Calling the trampoline function with anything except the `void*` pointer
/// will result in *Undefined Behaviour*.
///
/// The closure should guarantee that it never panics, seeing as panicking
/// across the FFI barrier is *Undefined Behaviour*. You may find
/// `std::panic::catch_unwind()` useful.
unsafe fn unpack_closure<F>(closure: *mut F) -> (*mut c_void, Callback)
where
  F: FnMut(),
{
  extern "C" fn trampoline<F>(data: *mut c_void) -> i32
  where
    F: FnMut(),
  {
    let result = panic::catch_unwind(move || {
      let mut closure: Box<F> = unsafe { Box::from_raw(data as *mut F) };
      closure();
    });
    if let Err(error) = result {
      error!("Error while invoking timer callback: {:?}", error);
    }
    0
  }

  (closure as *mut F as *mut c_void, trampoline::<F>)
}

/// A wrapper around wl_event_loop timers to call a handler after a
/// specified timeout.
pub(crate) struct WlTimer(*mut wl_event_source, Option<Box<dyn FnOnce()>>);

impl WlTimer {
  pub(crate) unsafe fn init<F>(
    display: *mut wl_display,
    timeout_ms: u32,
    handler: F,
  ) -> Result<WlTimer, ()>
  where
    F: FnMut(),
    F: 'static,
  {
    let handler_ptr = Box::into_raw(Box::new(handler));
    let drop_handler = Box::new(move || {
      Box::from_raw(handler_ptr);
    });
    let (closure, callback) = unpack_closure(handler_ptr);

    let event_loop = ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_get_event_loop, display);
    let timer = ffi_dispatch!(
      WAYLAND_SERVER_HANDLE,
      wl_event_loop_add_timer,
      event_loop,
      callback,
      closure
    );
    if timer.is_null() {
      drop_handler();
      return Err(());
    }
    let success = ffi_dispatch!(
      WAYLAND_SERVER_HANDLE,
      wl_event_source_timer_update,
      timer,
      timeout_ms as i32
    );
    if success < 0 {
      drop_handler();
      return Err(());
    }

    Ok(WlTimer(timer, Some(drop_handler)))
  }
}

impl Drop for WlTimer {
  fn drop(&mut self) {
    if !self.0.is_null() {
      unsafe {
        ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_event_source_remove, self.0);
      }
    }
    if let Some(drop) = self.1.take() {
      drop();
    }
  }
}
