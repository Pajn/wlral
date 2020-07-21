use super::device::Device;
use crate::{event::Event, window::Window};
use log::debug;
use std::cell::RefCell;
use std::pin::Pin;
use std::{ptr, rc::Rc};
use wlroots_sys::*;

mod wl_seat_capability {
  pub const WL_SEAT_CAPABILITY_POINTER: u32 = 1;
  pub const WL_SEAT_CAPABILITY_KEYBOARD: u32 = 2;
  #[allow(unused)]
  pub const WL_SEAT_CAPABILITY_TOUCH: u32 = 4;
}
use wl_seat_capability::*;

pub(crate) trait SeatEventHandler {
  fn new_input(&self, device_ptr: *mut wlr_input_device);
  fn inhibit_activate(&self);
  fn inhibit_deactivate(&self);
}

wayland_listener!(
  pub(crate) SeatEventManager,
  Box<dyn SeatEventHandler>,
  [
     new_input => new_input_func: |this: &mut SeatEventManager, data: *mut libc::c_void,| unsafe {
         let handler = &mut this.data;
         handler.new_input(data as _)
     };
     inhibit_activate => inhibit_activate_func: |this: &mut SeatEventManager, _data: *mut libc::c_void,| unsafe {
         let handler = &mut this.data;
         handler.inhibit_activate()
     };
     inhibit_deactivate => inhibit_deactivate_func: |this: &mut SeatEventManager, _data: *mut libc::c_void,| unsafe {
         let handler = &mut this.data;
         handler.inhibit_deactivate()
     };
  ]
);

pub struct SeatManager {
  pub(crate) seat: *mut wlr_seat,
  pub(crate) inhibit: *mut wlr_input_inhibit_manager,

  pub(crate) has_any_pointer: RefCell<bool>,
  pub(crate) has_any_keyboard: RefCell<bool>,
  pub(crate) exclusive_client: RefCell<*mut wl_client>,
  pub(crate) on_new_device: Event<Rc<Device>>,

  pub(crate) event_manager: RefCell<Option<Pin<Box<SeatEventManager>>>>,
}

impl SeatManager {
  pub(crate) fn init(
    display: *mut wl_display,
    backend: *mut wlr_backend,
    seat: *mut wlr_seat,
  ) -> Rc<SeatManager> {
    debug!("SeatManager::init");

    let inhibit = unsafe { wlr_input_inhibit_manager_create(display) };

    let seat_manager = Rc::new(SeatManager {
      seat,
      inhibit,

      has_any_pointer: RefCell::new(false),
      has_any_keyboard: RefCell::new(false),
      exclusive_client: RefCell::new(ptr::null_mut()),
      on_new_device: Event::default(),

      event_manager: RefCell::new(None),
    });

    let mut event_manager = SeatEventManager::new(Box::new(seat_manager.clone()));
    unsafe {
      event_manager.new_input(&mut (*backend).events.new_input);
      event_manager.inhibit_activate(&mut (*inhibit).events.activate);
      event_manager.inhibit_deactivate(&mut (*inhibit).events.deactivate);
    }
    *seat_manager.event_manager.borrow_mut() = Some(event_manager);

    seat_manager
  }

  #[cfg(test)]
  pub(crate) fn mock(
    seat: *mut wlr_seat,
    inhibit: *mut wlr_input_inhibit_manager,
  ) -> Rc<SeatManager> {
    Rc::new(SeatManager {
      seat,
      inhibit,

      has_any_pointer: RefCell::new(false),
      has_any_keyboard: RefCell::new(false),
      exclusive_client: RefCell::new(ptr::null_mut()),
      on_new_device: Event::default(),

      event_manager: RefCell::new(None),
    })
  }

  pub fn raw_seat(&self) -> *mut wlr_seat {
    self.seat
  }

  fn update_capabilities(&self) {
    let mut caps = 0;
    if *self.has_any_pointer.borrow() {
      caps |= WL_SEAT_CAPABILITY_POINTER;
    }
    if *self.has_any_keyboard.borrow() {
      caps |= WL_SEAT_CAPABILITY_KEYBOARD;
    }

    unsafe {
      wlr_seat_set_capabilities(self.seat, caps);
    }
  }

  pub(crate) fn set_has_any_pointer(&self, has_any_pointer: bool) {
    *self.has_any_pointer.borrow_mut() = has_any_pointer;
    self.update_capabilities();
  }

  pub(crate) fn set_has_any_keyboard(&self, has_any_keyboard: bool) {
    *self.has_any_keyboard.borrow_mut() = has_any_keyboard;
    self.update_capabilities();
  }

  fn set_exclusive_client(&self, exclusive_client: *mut wl_client) {
    if !exclusive_client.is_null() {
      // Clear keyboard focus
      unsafe {
        if !(*self.seat).keyboard_state.focused_client.is_null()
          && (*(*self.seat).keyboard_state.focused_client).client != exclusive_client
        {
          wlr_seat_keyboard_clear_focus(self.seat);
        }
      }

      // Clear pointer focus
      unsafe {
        if !(*self.seat).pointer_state.focused_client.is_null()
          && (*(*self.seat).pointer_state.focused_client).client != exclusive_client
        {
          // TODO: Change to wlr_seat_pointer_notify_clear_focus after updating wlroots
          wlr_seat_pointer_clear_focus(self.seat);
        }
      }
    }

    *self.exclusive_client.borrow_mut() = exclusive_client;
  }

  pub(crate) fn is_input_allowed(&self, window: &Window) -> bool {
    let exclusive_client = *self.exclusive_client.borrow();
    exclusive_client.is_null() || exclusive_client == window.wl_client()
  }
}

impl SeatEventHandler for Rc<SeatManager> {
  fn new_input(&self, device_ptr: *mut wlr_input_device) {
    debug!("SeatManager::new_input");
    let device = Device::init(device_ptr);

    self.on_new_device.fire(device);
  }
  fn inhibit_activate(&self) {
    debug!("LayersEventHandler::inhibit_activate");
    unsafe {
      self.set_exclusive_client((*self.inhibit).active_client);
    }
  }
  fn inhibit_deactivate(&self) {
    debug!("LayersEventHandler::inhibit_deactivate");
    self.set_exclusive_client(ptr::null_mut());
  }
}

#[cfg(test)]
unsafe fn wlr_seat_set_capabilities(_: *mut wlr_seat, _: u32) {}
#[cfg(test)]
unsafe fn wlr_input_inhibit_manager_create(_: *mut wl_display) -> *mut wlr_input_inhibit_manager {
  ptr::null_mut()
}
