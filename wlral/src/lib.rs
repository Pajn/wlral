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
mod tests {
  #[test]
  fn it_works() {
    assert_eq!(2 + 2, 4);
  }
}
