use std::cell::RefCell;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

mod wl_seat_capability {
  pub const WL_SEAT_CAPABILITY_POINTER: u32 = 1;
  pub const WL_SEAT_CAPABILITY_KEYBOARD: u32 = 2;
  #[allow(unused)]
  pub const WL_SEAT_CAPABILITY_TOUCH: u32 = 4;
}
use wl_seat_capability::*;

#[derive(Debug, PartialEq)]
pub enum DeviceType {
  Keyboard(*mut wlr_keyboard),
  Pointer(*mut wlr_pointer),
  Unknown,
}

pub struct Device {
  device: *mut wlr_input_device,
  seat_event_manager: Rc<SeatEventHandler>,

  event_manager: RefCell<Option<Pin<Box<DeviceEventManager>>>>,
}

impl Device {
  pub(crate) fn init(
    seat_event_manager: Rc<SeatEventHandler>,
    device: *mut wlr_input_device,
  ) -> Rc<Device> {
    let device = Rc::new(Device {
      device,
      seat_event_manager,
      event_manager: RefCell::new(None),
    });

    let mut event_manager = DeviceEventManager::new(Rc::downgrade(&device));
    unsafe {
      event_manager.destroy(&mut (*device.raw_ptr()).events.destroy);
    }
    *device.event_manager.borrow_mut() = Some(event_manager);

    device
  }

  fn destroy(&self) {
    self.seat_event_manager.destroy_device(self);
  }

  pub fn device_type(&self) -> DeviceType {
    unsafe {
      let device = &*self.device;
      match device.type_ {
        type_ if type_ == wlr_input_device_type_WLR_INPUT_DEVICE_KEYBOARD => {
          DeviceType::Keyboard(device.__bindgen_anon_1.keyboard)
        }
        type_ if type_ == wlr_input_device_type_WLR_INPUT_DEVICE_POINTER => {
          DeviceType::Pointer(device.__bindgen_anon_1.pointer)
        }
        _ => DeviceType::Unknown,
      }
    }
  }

  pub fn raw_ptr(&self) -> *mut wlr_input_device {
    self.device
  }
}

impl PartialEq for Device {
  fn eq(&self, other: &Device) -> bool {
    self.device == other.device
  }
}

wayland_listener!(
  pub DeviceEventManager,
  Weak<Device>,
  [
    destroy => destroy_func: |this: &mut DeviceEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.destroy();
      }
    };
  ]
);

pub(crate) trait InputDeviceManager {
  fn has_any_input_device(&self) -> bool;
  fn add_input_device(&mut self, device: Rc<Device>);
  fn destroy_input_device(&mut self, destroyed_device: &Device);
}

pub struct SeatEventHandler {
  pub(crate) seat: *mut wlr_seat,

  pub(crate) cursor_manager: Rc<RefCell<dyn InputDeviceManager>>,
  pub(crate) keyboard_manager: Rc<RefCell<dyn InputDeviceManager>>,
}

impl SeatEventHandler {
  fn update_capabilities(&self) {
    let mut caps = 0;
    if self.cursor_manager.borrow().has_any_input_device() {
      caps |= WL_SEAT_CAPABILITY_POINTER;
    }
    if self.keyboard_manager.borrow().has_any_input_device() {
      caps |= WL_SEAT_CAPABILITY_KEYBOARD;
    }

    unsafe {
      wlr_seat_set_capabilities(self.seat, caps);
    }
  }

  fn destroy_device(&self, device: &Device) {
    match device.device_type() {
      DeviceType::Keyboard(_) => {
        self
          .keyboard_manager
          .borrow_mut()
          .destroy_input_device(device);
      }
      DeviceType::Pointer(_) => {
        self
          .cursor_manager
          .borrow_mut()
          .destroy_input_device(device);
      }
      _ => {}
    }

    self.update_capabilities();
  }
}

trait SeatEventHandlerExt {
  fn new_input(&self, device_ptr: *mut wlr_input_device);
}

impl SeatEventHandlerExt for Rc<SeatEventHandler> {
  fn new_input(&self, device_ptr: *mut wlr_input_device) {
    let device = Device::init(self.clone(), device_ptr);

    match device.device_type() {
      DeviceType::Keyboard(_) => {
        self.keyboard_manager.borrow_mut().add_input_device(device);

        unsafe {
          wlr_seat_set_keyboard(self.seat, device_ptr);
        }
      }
      DeviceType::Pointer(_) => {
        self.cursor_manager.borrow_mut().add_input_device(device);
      }
      _ => {}
    }

    self.update_capabilities();
  }
}

wayland_listener!(
  pub SeatEventManager,
  Rc<SeatEventHandler>,
  [
     new_input => new_input_func: |this: &mut SeatEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.new_input(data as _)
     };
  ]
);

#[allow(unused)]
pub(crate) struct SeatManager {
  seat: *mut wlr_seat,

  event_manager: Pin<Box<SeatEventManager>>,
  event_handler: Rc<SeatEventHandler>,
}

impl SeatManager {
  pub(crate) fn init(
    backend: *mut wlr_backend,
    seat: *mut wlr_seat,
    cursor_manager: Rc<RefCell<dyn InputDeviceManager>>,
    keyboard_manager: Rc<RefCell<dyn InputDeviceManager>>,
  ) -> SeatManager {
    println!("SeatManager::init prebind");

    let event_handler = Rc::new(SeatEventHandler {
      seat,

      cursor_manager,
      keyboard_manager,
    });

    let mut event_manager = SeatEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_input(&mut (*backend).events.new_input);
    }

    println!("SeatManager::init postbind");

    SeatManager {
      seat,

      event_manager,
      event_handler,
    }
  }
}

#[cfg(test)]
unsafe fn wlr_seat_set_capabilities(_: *mut wlr_seat, _: u32) {}
#[cfg(test)]
unsafe fn wlr_seat_set_keyboard(_: *mut wlr_seat, _: *mut wlr_input_device) {}
