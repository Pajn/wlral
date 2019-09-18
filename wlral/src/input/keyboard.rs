use std::cell::RefCell;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

pub struct Keyboard {
  seat: *mut wlr_seat,
  device: *mut wlr_input_device,
  keyboard: *mut wlr_keyboard,
  keyboard_manager: Rc<RefCell<KeyboardManager>>,

  #[allow(unused)]
  event_manager: RefCell<Option<Pin<Box<KeyboardEventManager>>>>,
}

impl Keyboard {
  fn init(
    keyboard_manager: Rc<RefCell<KeyboardManager>>,
    device: *mut wlr_input_device,
  ) -> Rc<Keyboard> {
    unsafe {
      if (*device).type_ != wlr_input_device_type_WLR_INPUT_DEVICE_KEYBOARD {
        panic!("Keyboard::init expects a keyboard device");
      }
      let seat = keyboard_manager.borrow().seat;
      let keyboard_ptr = (*device).__bindgen_anon_1.keyboard;
      let keyboard = Rc::new(Keyboard {
        seat,
        device,
        keyboard: keyboard_ptr,
        keyboard_manager,
        event_manager: RefCell::new(None),
      });

      #[cfg(not(test))]
      {
        use std::ptr;

        // We need to prepare an XKB keymap and assign it to the keyboard.
        // This assumes the defaults (e.g. layout = "us").
        let rules = xkb_rule_names {
          rules: ptr::null(),
          model: ptr::null(),
          layout: ptr::null(),
          variant: ptr::null(),
          options: ptr::null(),
        };

        let context = xkb_context_new(xkb_context_flags_XKB_CONTEXT_NO_FLAGS);
        let keymap =
          xkb_keymap_new_from_names(context, &rules, xkb_context_flags_XKB_CONTEXT_NO_FLAGS);

        wlr_keyboard_set_keymap(keyboard_ptr, keymap);
        xkb_keymap_unref(keymap);
        xkb_context_unref(context);
        wlr_keyboard_set_repeat_info(keyboard_ptr, 25, 600);
      }

      println!("Keyboard::init prebind");

      let mut event_manager = KeyboardEventManager::new(Rc::downgrade(&keyboard));
      event_manager.modifiers(&mut (*keyboard_ptr).events.modifiers);
      event_manager.key(&mut (*keyboard_ptr).events.key);
      event_manager.destroy(&mut (*device).events.destroy);
      *keyboard.event_manager.borrow_mut() = Some(event_manager);

      println!("Keyboard::init postbind");

      keyboard
    }
  }
}

pub trait KeyboardEventHandler {
  fn modifiers(&self);
  fn key(&self, event: *mut wlr_event_keyboard_key);
  fn destroy(&self);
}

impl KeyboardEventHandler for Keyboard {
  fn modifiers(&self) {
    unsafe {
      // A seat can only have one keyboard, but this is a limitation of the
      // Wayland protocol - not wlroots. We assign all connected keyboards to the
      // same seat. You can swap out the underlying wlr_keyboard like this and
      // wlr_seat handles this transparently.
      wlr_seat_set_keyboard(self.seat, self.device);
      // Send modifiers to the client.
      wlr_seat_keyboard_notify_modifiers(self.seat, &mut (*self.keyboard).modifiers);
    }
  }

  fn key(&self, event: *mut wlr_event_keyboard_key) {
    unsafe {
      // Translate libinput keycode -> xkbcommon
      // let keycode = (*event).keycode + 8;
      // Get a list of keysyms based on the keymap for this keyboard
      // let mut syms: xkb_keysym_t = 0;
      // let nsyms = xkb_state_key_get_syms(
      //     (*self.keyboard).xkb_state, keycode, &mut &syms);

      let handled = false;

      if !handled {
        // Otherwise, we pass it along to the client.
        wlr_seat_set_keyboard(self.seat, self.device);
        wlr_seat_keyboard_notify_key(
          self.seat,
          (*event).time_msec,
          (*event).keycode,
          (*event).state,
        );
      }
    }
  }

  fn destroy(&self) {
    self.keyboard_manager.borrow_mut().destroy_keyboard(self);
  }
}

wayland_listener!(
  pub KeyboardEventManager,
  Weak<Keyboard>,
  [
    modifiers => modifiers_func: |this: &mut KeyboardEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.modifiers()
      }
    };
    key => key_func: |this: &mut KeyboardEventManager, data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.key(data as _)
      }
    };
    destroy => destroy_func: |this: &mut KeyboardEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.destroy()
      }
    };
  ]
);

pub struct KeyboardManager {
  seat: *mut wlr_seat,
  keyboards: Vec<Rc<Keyboard>>,
}

impl KeyboardManager {
  pub fn init(seat: *mut wlr_seat) -> KeyboardManager {
    KeyboardManager {
      seat,
      keyboards: vec![],
    }
  }

  pub fn has_keyboard_device(&self) -> bool {
    !self.keyboards.is_empty()
  }

  pub fn destroy_keyboard(&mut self, destroyed_keyboard: &Keyboard) {
    self
      .keyboards
      .retain(|keyboard| keyboard.keyboard != destroyed_keyboard.keyboard);
  }
}

pub trait KeyboardManagerExt {
  fn add_keyboard_device(&self, device: *mut wlr_input_device) -> Rc<Keyboard>;
}

impl KeyboardManagerExt for Rc<RefCell<KeyboardManager>> {
  fn add_keyboard_device(&self, device: *mut wlr_input_device) -> Rc<Keyboard> {
    let keyboard = Keyboard::init(self.clone(), device);
    self.borrow_mut().keyboards.push(keyboard.clone());
    keyboard
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
    let keyboard_manager = Rc::new(RefCell::new(KeyboardManager::init(ptr::null_mut())));

    let mut raw_keyboard = wlr_keyboard {
      impl_: ptr::null(),
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

    let keyboard = keyboard_manager.add_keyboard_device(&mut device);

    let weak_keyboard = Rc::downgrade(&keyboard);
    drop(keyboard);

    assert!(weak_keyboard.upgrade().is_some());
    assert!(key_signal.listener_count() == 1);
    assert!(modifiers_signal.listener_count() == 1);
    assert!(destroy_signal.listener_count() == 1);

    destroy_signal.emit();

    assert!(key_signal.listener_count() == 0);
    assert!(modifiers_signal.listener_count() == 0);
    assert!(keymap_signal.listener_count() == 0);
    assert!(repeat_info_signal.listener_count() == 0);
    assert!(destroy_signal.listener_count() == 0);
    assert!(keyboard_manager.borrow().keyboards.len() == 0);
    assert!(weak_keyboard.upgrade().is_none());
  }
}
