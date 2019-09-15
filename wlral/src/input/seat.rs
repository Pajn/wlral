use crate::input::cursor::CursorManager;
use crate::input::keyboard::KeyboardManager;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::signal::wl_signal_add;
use wlroots_sys::*;

mod wl_seat_capability {
  pub const WL_SEAT_CAPABILITY_POINTER: u32 = 1;
  pub const WL_SEAT_CAPABILITY_KEYBOARD: u32 = 2;
  #[allow(unused)]
  pub const WL_SEAT_CAPABILITY_TOUCH: u32 = 4;
}
use wl_seat_capability::*;

pub struct SeatEventHandler {
  seat: *mut wlr_seat,

  cursor_manager: Rc<RefCell<CursorManager>>,
  keyboard_manager: Rc<RefCell<KeyboardManager>>,
}

impl SeatEventHandler {
  fn new_input(&mut self, device: *mut wlr_input_device) {
    unsafe {
      if (*device).type_ == wlr_input_device_type_WLR_INPUT_DEVICE_POINTER {
        self.cursor_manager.borrow_mut().add_pointer_device(device);
      } else if (*device).type_ == wlr_input_device_type_WLR_INPUT_DEVICE_KEYBOARD {
        self
          .keyboard_manager
          .borrow_mut()
          .add_keyboard_device(device);

        wlr_seat_set_keyboard(self.seat, device);
      }

      let mut caps = 0;
      if self.cursor_manager.borrow().has_pointer_device() {
        caps |= WL_SEAT_CAPABILITY_POINTER;
      }
      if self.keyboard_manager.borrow().has_keyboard_device() {
        caps |= WL_SEAT_CAPABILITY_KEYBOARD;
      }

      wlr_seat_set_capabilities(self.seat, caps);
    }
  }
}

wayland_listener!(
  pub SeatEventManager,
  Rc<RefCell<SeatEventHandler>>,
  [
     new_input => new_input_func: |this: &mut SeatEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().new_input(data as _)
     };
  ]
);

#[allow(unused)]
pub struct SeatManager {
  seat: *mut wlr_seat,

  event_manager: Pin<Box<SeatEventManager>>,
  event_handler: Rc<RefCell<SeatEventHandler>>,
}

impl SeatManager {
  pub fn init(
    backend: *mut wlr_backend,
    seat: *mut wlr_seat,
    cursor_manager: Rc<RefCell<CursorManager>>,
    keyboard_manager: Rc<RefCell<KeyboardManager>>,
  ) -> SeatManager {
    println!("SeatManager::init prebind");

    let event_handler = Rc::new(RefCell::new(SeatEventHandler {
      seat,

      cursor_manager,
      keyboard_manager,
    }));

    let mut event_manager = SeatEventManager::new(event_handler.clone());
    unsafe {
      wl_signal_add(&mut (*backend).events.new_input, event_manager.new_input());
    }

    println!("SeatManager::init postbind");

    SeatManager {
      seat,

      event_manager,
      event_handler,
    }
  }
}
