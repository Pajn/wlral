use crate::event::EventOnce;
use log::debug;
use std::cell::RefCell;
use std::pin::Pin;
use std::{
  borrow::Cow,
  ffi::CStr,
  rc::{Rc, Weak},
};
use wlroots_sys::*;

#[derive(Debug, PartialEq)]
pub enum DeviceType {
  Keyboard(*mut wlr_keyboard),
  Pointer(*mut wlr_pointer),
  Unknown,
}

pub struct Device {
  device: *mut wlr_input_device,
  pub on_destroy: EventOnce<()>,

  event_manager: RefCell<Option<Pin<Box<DeviceEventManager>>>>,
}

impl Device {
  pub(crate) fn init(device: *mut wlr_input_device) -> Rc<Device> {
    let device = Rc::new(Device {
      device,
      on_destroy: EventOnce::default(),

      event_manager: RefCell::new(None),
    });

    let mut event_manager = DeviceEventManager::new(Rc::downgrade(&device));
    unsafe {
      event_manager.destroy(&mut (*device.raw_ptr()).events.destroy);
    }
    *device.event_manager.borrow_mut() = Some(event_manager);

    device
  }

  pub fn raw_ptr(&self) -> *mut wlr_input_device {
    self.device
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

  pub fn name(&self) -> Cow<str> {
    unsafe { CStr::from_ptr((*self.device).name).to_string_lossy() }
  }

  pub fn output_name(&self) -> Option<Cow<str>> {
    unsafe {
      let output_name = (*self.device).output_name;
      if output_name.is_null() {
        None
      } else {
        Some(CStr::from_ptr(output_name).to_string_lossy())
      }
    }
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
        debug!("Device::destroy");
        handler.on_destroy.fire(())
      }
    };
  ]
);
