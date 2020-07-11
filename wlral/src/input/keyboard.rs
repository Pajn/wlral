use crate::input::device::{Device, DeviceType};
use crate::input::event_filter::{EventFilter, EventFilterManager};
use crate::input::events::{InputEvent, KeyboardEvent};
use crate::input::seat::SeatManager;
use log::debug;
use std::cell::RefCell;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use wlroots_sys::*;
use xkbcommon::xkb;
#[cfg(not(test))]
use xkbcommon::xkb::ffi::xkb_state_ref;

pub struct Keyboard {
  seat_manager: Rc<SeatManager>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
  device: Rc<Device>,
  keyboard: *mut wlr_keyboard,
  xkb_state: xkb::State,

  event_manager: RefCell<Option<Pin<Box<KeyboardEventManager>>>>,
}

impl Keyboard {
  fn init(
    seat_manager: Rc<SeatManager>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
    device: Rc<Device>,
  ) -> Rc<Keyboard> {
    debug!("Keyboard::init");

    let keyboard_ptr = match device.device_type() {
      DeviceType::Keyboard(keyboard_ptr) => keyboard_ptr,
      _ => panic!("Keyboard::init expects a keyboard device"),
    };

    // We need to prepare an XKB keymap and assign it to the keyboard.
    // This assumes the defaults (e.g. layout = "us").
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
      &context,
      "", // rules
      "", // model
      "", // layout
      "", // variant
      None,
      xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .expect("xkb::Keymap could not be created");

    unsafe {
      wlr_keyboard_set_keymap(keyboard_ptr, keymap.get_raw_ptr());
      wlr_keyboard_set_repeat_info(keyboard_ptr, 25, 600);
    }

    let keyboard = Rc::new(Keyboard {
      seat_manager,
      event_filter_manager,
      device,
      keyboard: keyboard_ptr,
      xkb_state: unsafe { xkb::State::from_raw_ptr(xkb_state_ref((*keyboard_ptr).xkb_state)) },
      event_manager: RefCell::new(None),
    });

    let mut event_manager = KeyboardEventManager::new(Rc::downgrade(&keyboard));
    unsafe {
      event_manager.modifiers(&mut (*keyboard_ptr).events.modifiers);
      event_manager.key(&mut (*keyboard_ptr).events.key);
    }
    *keyboard.event_manager.borrow_mut() = Some(event_manager);

    keyboard
  }

  pub fn device(&self) -> Rc<Device> {
    self.device.clone()
  }

  pub fn xkb_state(&self) -> &xkb::State {
    &self.xkb_state
  }
}

pub trait KeyboardEventHandler {
  fn modifiers(&self);
  fn key(&self, event: *const wlr_event_keyboard_key);
}

impl KeyboardEventHandler for Keyboard {
  fn modifiers(&self) {
    unsafe {
      // A seat can only have one keyboard, but this is a limitation of the
      // Wayland protocol - not wlroots. We assign all connected keyboards to the
      // same seat. You can swap out the underlying wlr_keyboard like this and
      // wlr_seat handles this transparently.
      wlr_seat_set_keyboard(self.seat_manager.raw_seat(), self.device.raw_ptr());
      // Send modifiers to the client.
      wlr_seat_keyboard_notify_modifiers(
        self.seat_manager.raw_seat(),
        &mut (*self.keyboard).modifiers,
      );
    }
  }

  fn key(&self, event: *const wlr_event_keyboard_key) {
    let event = unsafe { KeyboardEvent::from_ptr(self, event) };

    let handled = self
      .event_filter_manager
      .borrow_mut()
      .handle_keyboard_event(&event);

    if !handled {
      unsafe {
        // Otherwise, we pass it along to the client.
        wlr_seat_set_keyboard(self.seat_manager.raw_seat(), self.device.raw_ptr());
        wlr_seat_keyboard_notify_key(
          self.seat_manager.raw_seat(),
          event.time_msec(),
          event.libinput_keycode(),
          event.state(),
        );
      }
    }
  }
}

wayland_listener!(
  pub KeyboardEventManager,
  Weak<Keyboard>,
  [
    modifiers => modifiers_func: |this: &mut KeyboardEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.modifiers();
      }
    };
    key => key_func: |this: &mut KeyboardEventManager, data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.key(data as _);
      }
    };
  ]
);

pub struct KeyboardManager {
  seat_manager: Rc<SeatManager>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
  keyboards: RefCell<Vec<Rc<Keyboard>>>,
}

impl KeyboardManager {
  pub(crate) fn init(
    seat_manager: Rc<SeatManager>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
  ) -> Rc<KeyboardManager> {
    let keyboard_manager = Rc::new(KeyboardManager {
      seat_manager: seat_manager.clone(),
      event_filter_manager,
      keyboards: RefCell::new(vec![]),
    });

    seat_manager
      .on_new_device
      .subscribe(listener!(keyboard_manager => move |device| {
        if let DeviceType::Keyboard(_) = device.device_type() {
          device.on_destroy.then(listener!(device, keyboard_manager => move || {
            keyboard_manager
              .keyboards
              .borrow_mut()
              .retain(|keyboard| keyboard.device.deref() != device.deref());

              keyboard_manager
              .seat_manager
              .set_has_any_keyboard(keyboard_manager.has_keyboard());
          }));

          unsafe {
            wlr_seat_set_keyboard(keyboard_manager.seat_manager.raw_seat(), device.raw_ptr());
          }
          let keyboard = Keyboard::init(
            keyboard_manager.seat_manager.clone(),
            keyboard_manager.event_filter_manager.clone(),
            device.clone(),
          );
          keyboard_manager.keyboards.borrow_mut().push(keyboard);
          keyboard_manager.seat_manager.set_has_any_keyboard(true);
        }
      }));

    keyboard_manager
  }

  pub fn has_keyboard(&self) -> bool {
    !self.keyboards.borrow().is_empty()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_util::*;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let seat_manager = SeatManager::mock(ptr::null_mut(), ptr::null_mut());
    let event_filter_manager = Rc::new(RefCell::new(EventFilterManager::new()));
    let keyboard_manager = Rc::new(KeyboardManager::init(
      seat_manager.clone(),
      event_filter_manager,
    ));

    let mut raw_keyboard = wlr_keyboard {
      impl_: ptr::null(),
      group: ptr::null_mut(),
      keymap_string: ptr::null_mut(),
      keymap_size: 0,
      keymap: ptr::null_mut(),
      xkb_state: ptr::null_mut(),
      led_indexes: [0; 3],
      mod_indexes: [0; 8],
      keycodes: [0; 32],
      num_keycodes: 0,
      modifiers: wlr_keyboard_modifiers {
        depressed: 0,
        latched: 0,
        locked: 0,
        group: 0,
      },
      repeat_info: wlr_keyboard__bindgen_ty_1 { rate: 0, delay: 0 },
      events: wlr_keyboard__bindgen_ty_2 {
        key: new_wl_signal(),
        modifiers: new_wl_signal(),
        keymap: new_wl_signal(),
        repeat_info: new_wl_signal(),
        destroy: new_wl_signal(),
      },
      data: ptr::null_mut(),
    };
    let mut device = wlr_input_device {
      impl_: ptr::null(),
      type_: wlr_input_device_type_WLR_INPUT_DEVICE_KEYBOARD,
      vendor: 0,
      product: 0,
      name: ptr::null_mut(),
      width_mm: 0.0,
      height_mm: 0.0,
      output_name: ptr::null_mut(),
      __bindgen_anon_1: wlr_input_device__bindgen_ty_1 {
        keyboard: &mut raw_keyboard,
      },
      events: wlr_input_device__bindgen_ty_2 {
        destroy: new_wl_signal(),
      },
      data: ptr::null_mut(),
      link: new_wl_list(),
    };

    let key_signal = WlSignal::from_ptr(&mut raw_keyboard.events.key);
    let modifiers_signal = WlSignal::from_ptr(&mut raw_keyboard.events.modifiers);
    let keymap_signal = WlSignal::from_ptr(&mut raw_keyboard.events.keymap);
    let repeat_info_signal = WlSignal::from_ptr(&mut raw_keyboard.events.repeat_info);
    let destroy_signal = WlSignal::from_ptr(&mut device.events.destroy);

    let device = Device::init(&mut device);
    let weak_device = Rc::downgrade(&device);
    seat_manager.on_new_device.fire(device);
    let keyboard = keyboard_manager.keyboards.borrow().first().unwrap().clone();

    let weak_keyboard = Rc::downgrade(&keyboard);
    drop(keyboard);

    assert!(weak_device.upgrade().is_some());
    assert!(weak_keyboard.upgrade().is_some());
    assert!(key_signal.listener_count() == 1);
    assert!(modifiers_signal.listener_count() == 1);
    assert!(destroy_signal.listener_count() == 1);
    assert!(keyboard_manager.has_keyboard());

    destroy_signal.emit();

    assert!(key_signal.listener_count() == 0);
    assert!(modifiers_signal.listener_count() == 0);
    assert!(keymap_signal.listener_count() == 0);
    assert!(repeat_info_signal.listener_count() == 0);
    assert!(destroy_signal.listener_count() == 0);
    assert!(!keyboard_manager.has_keyboard());
    assert!(weak_keyboard.upgrade().is_none());
    assert!(weak_device.upgrade().is_none());
  }
}

#[cfg(test)]
use xkbcommon::xkb::ffi::{xkb_keymap, xkb_state};
#[cfg(test)]
unsafe fn wlr_seat_set_keyboard(_: *mut wlr_seat, _: *mut wlr_input_device) {}
#[cfg(test)]
unsafe fn wlr_keyboard_set_keymap(_: *mut wlr_keyboard, _: *mut xkb_keymap) {}
#[cfg(test)]
unsafe fn wlr_keyboard_set_repeat_info(_: *mut wlr_keyboard, _: u32, _: u32) {}
#[cfg(test)]
unsafe fn xkb_state_ref(ptr: *mut xkb_state) -> *mut xkb_state {
  ptr
}
