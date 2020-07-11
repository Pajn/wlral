#![allow(non_camel_case_types, non_upper_case_globals)]
#![allow(clippy::all)]

pub extern crate libc;
pub extern crate wayland_commons;
pub extern crate wayland_server;
pub extern crate wayland_sys;

pub use wayland_sys::{
  gid_t, pid_t,
  server::{self, WAYLAND_SERVER_HANDLE, *},
  uid_t, *,
};

#[allow(
  non_camel_case_types,
  non_snake_case,
  non_upper_case_globals,
  invalid_value,
  unused
)]
mod generated {
  use libc;
  include!("gen.rs");
}
pub use self::generated::*;

#[cfg(feature = "unstable")]
pub type wlr_output_events = self::generated::wlr_output__bindgen_ty_1;
#[cfg(feature = "unstable")]
pub type wlr_input_device_pointer = self::generated::wlr_input_device__bindgen_ty_1;
