use std::cell::RefCell;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wlroots_sys::*;

pub struct Keyboard {
  seat: *mut wlr_seat,
  device: *mut wlr_input_device,
  keyboard: *mut wlr_keyboard,

  #[allow(unused)]
  event_manager: RefCell<Option<Pin<Box<KeyboardEventManager>>>>,
}

impl Keyboard {
  fn init(seat: *mut wlr_seat, device: *mut wlr_input_device) -> Rc<Keyboard> {
    unsafe {
      if (*device).type_ != wlr_input_device_type_WLR_INPUT_DEVICE_KEYBOARD {
        panic!("Keyboard::init expects a keyboard device");
      }
      let keyboard_ptr = (*device).__bindgen_anon_1.keyboard;
      let keyboard = Rc::new(Keyboard {
        seat,
        device,
        keyboard: keyboard_ptr,
        event_manager: RefCell::new(None),
      });

      // We need to prepare an XKB keymap and assign it to the keyboard. This
      // assumes the defaults (e.g. layout = "us").
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

      println!("Keyboard::init prebind");

      let mut event_manager = KeyboardEventManager::new(keyboard.clone());
      event_manager.modifiers(&mut (*keyboard_ptr).events.modifiers);
      event_manager.key(&mut (*keyboard_ptr).events.key);
      *keyboard.event_manager.borrow_mut() = Some(event_manager);

      println!("Keyboard::init postbind");

      keyboard
    }
  }
}

pub trait KeyboardEventHandler {
  fn modifiers(&self);
  fn key(&self, event: *mut wlr_event_keyboard_key);
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
}

wayland_listener!(
  pub KeyboardEventManager,
  Rc<Keyboard>,
  [
     modifiers => modifiers_func: |this: &mut KeyboardEventManager, _data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.modifiers()
     };
     key => key_func: |this: &mut KeyboardEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.key(data as _)
     };
  ]
);

#[allow(unused)]
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

  pub fn add_keyboard_device(&mut self, device: *mut wlr_input_device) {
    let keyboard = Keyboard::init(self.seat, device);
    self.keyboards.push(keyboard);
  }
}
