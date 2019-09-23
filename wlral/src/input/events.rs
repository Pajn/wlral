use crate::geometry::{FDisplacement, FPoint};
use crate::input::cursor::CursorManager;
use crate::input::keyboard::Keyboard;
use std::cell::RefCell;
use std::rc::Rc;
use wlroots_sys::*;
use xkbcommon::xkb;

// NOTE Taken from linux/input-event-codes.h
// TODO Find a way to automatically parse and fetch from there.
pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;
pub const BTN_SIDE: u32 = 0x113;
pub const BTN_EXTRA: u32 = 0x114;
pub const BTN_FORWARD: u32 = 0x115;
pub const BTN_BACK: u32 = 0x116;
pub const BTN_TASK: u32 = 0x117;

pub trait InputEvent {
  /// Get the timestamp of this event
  fn time_msec(&self) -> u32;

  /// Get the raw pointer to the device that fired this event
  fn raw_device(&self) -> *mut wlr_input_device;
}

pub trait CursorEvent {
  /// Get the position of the cursor in global coordinates
  fn position(&self) -> FPoint;
}

/// Event that triggers when the pointer device scrolls (e.g using a wheel
/// or in the case of a touchpad when you use two fingers to scroll)
pub struct AxisEvent {
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  event: *const wlr_event_pointer_axis,
}

impl AxisEvent {
  pub(crate) unsafe fn from_ptr(
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    event: *const wlr_event_pointer_axis,
  ) -> Self {
    AxisEvent {
      cursor_manager,
      event,
    }
  }

  /// Get the raw pointer to this event
  pub fn raw_event(&self) -> *const wlr_event_pointer_axis {
    self.event
  }

  pub fn source(&self) -> wlr_axis_source {
    unsafe { (*self.event).source }
  }

  pub fn orientation(&self) -> wlr_axis_orientation {
    unsafe { (*self.event).orientation }
  }

  /// Get the change from the last axis value
  ///
  /// Useful to determine e.g how much to scroll.
  pub fn delta(&self) -> f64 {
    unsafe { (*self.event).delta }
  }

  pub fn delta_discrete(&self) -> i32 {
    unsafe { (*self.event).delta_discrete }
  }
}

impl InputEvent for AxisEvent {
  fn raw_device(&self) -> *mut wlr_input_device {
    unsafe { (*self.event).device }
  }

  fn time_msec(&self) -> u32 {
    unsafe { (*self.event).time_msec }
  }
}

impl CursorEvent for AxisEvent {
  fn position(&self) -> FPoint {
    self.cursor_manager.borrow().position()
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ButtonState {
  Released,
  Pressed,
}

impl ButtonState {
  pub fn from_raw(state: wlr_button_state) -> ButtonState {
    if state == wlr_button_state_WLR_BUTTON_RELEASED {
      ButtonState::Released
    } else {
      ButtonState::Pressed
    }
  }

  pub fn as_raw(&self) -> wlr_button_state {
    match self {
      ButtonState::Released => wlr_button_state_WLR_BUTTON_RELEASED,
      ButtonState::Pressed => wlr_button_state_WLR_BUTTON_PRESSED,
    }
  }
}

/// Event that triggers when a button is pressed (e.g left click, right click,
/// a gaming mouse button, etc.)
pub struct ButtonEvent {
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  event: *const wlr_event_pointer_button,
}

impl ButtonEvent {
  pub(crate) unsafe fn from_ptr(
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    event: *const wlr_event_pointer_button,
  ) -> Self {
    ButtonEvent {
      cursor_manager,
      event,
    }
  }

  /// Get the raw pointer to this event
  pub fn raw_event(&self) -> *const wlr_event_pointer_button {
    self.event
  }

  /// Get the state of the button (e.g pressed or released)
  pub fn state(&self) -> ButtonState {
    ButtonState::from_raw(unsafe { (*self.event).state })
  }

  /// Get the value of the button pressed. This will generally be an
  /// atomically increasing value, with e.g left click being 1 and right
  /// click being 2...
  ///
  /// We make no guarantees that 1 always maps to left click, as this is
  /// device driver specific.
  pub fn button(&self) -> u32 {
    unsafe { (*self.event).button }
  }
}

impl InputEvent for ButtonEvent {
  fn raw_device(&self) -> *mut wlr_input_device {
    unsafe { (*self.event).device }
  }

  fn time_msec(&self) -> u32 {
    unsafe { (*self.event).time_msec }
  }
}

impl CursorEvent for ButtonEvent {
  fn position(&self) -> FPoint {
    self.cursor_manager.borrow().position()
  }
}

/// Event that triggers when the pointer moves
pub enum MotionEvent {
  Relative(RelativeMotionEvent),
  Absolute(AbsoluteMotionEvent),
}

impl InputEvent for MotionEvent {
  fn raw_device(&self) -> *mut wlr_input_device {
    match self {
      MotionEvent::Relative(event) => event.raw_device(),
      MotionEvent::Absolute(event) => event.raw_device(),
    }
  }

  fn time_msec(&self) -> u32 {
    match self {
      MotionEvent::Relative(event) => event.time_msec(),
      MotionEvent::Absolute(event) => event.time_msec(),
    }
  }
}

impl CursorEvent for MotionEvent {
  fn position(&self) -> FPoint {
    match self {
      MotionEvent::Relative(event) => event.position(),
      MotionEvent::Absolute(event) => event.position(),
    }
  }
}

pub struct RelativeMotionEvent {
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  event: *const wlr_event_pointer_motion,
}

impl RelativeMotionEvent {
  pub(crate) unsafe fn from_ptr(
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    event: *const wlr_event_pointer_motion,
  ) -> Self {
    RelativeMotionEvent {
      cursor_manager,
      event,
    }
  }

  /// Get the raw pointer to this event
  pub fn raw_event(&self) -> *const wlr_event_pointer_motion {
    self.event
  }

  /// Get the change from the last positional value
  ///
  /// Note you should not cast this to a type with less precision,
  /// otherwise you'll lose important motion data which can cause bugs
  /// (e.g see [this fun wlc bug](https://github.com/Cloudef/wlc/issues/181)).
  pub fn delta(&self) -> FDisplacement {
    unsafe {
      FDisplacement {
        dx: (*self.event).delta_x,
        dy: (*self.event).delta_y,
      }
    }
  }
}

impl InputEvent for RelativeMotionEvent {
  fn raw_device(&self) -> *mut wlr_input_device {
    unsafe { (*self.event).device }
  }

  fn time_msec(&self) -> u32 {
    unsafe { (*self.event).time_msec }
  }
}

impl CursorEvent for RelativeMotionEvent {
  fn position(&self) -> FPoint {
    self.cursor_manager.borrow().position()
  }
}

pub struct AbsoluteMotionEvent {
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  event: *const wlr_event_pointer_motion_absolute,
}

impl AbsoluteMotionEvent {
  pub(crate) unsafe fn from_ptr(
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    event: *const wlr_event_pointer_motion_absolute,
  ) -> Self {
    AbsoluteMotionEvent {
      cursor_manager,
      event,
    }
  }

  /// Get the raw pointer to this event
  pub fn raw_event(&self) -> *const wlr_event_pointer_motion_absolute {
    self.event
  }

  /// Get the absolute position of the pointer from this event
  pub fn pos(&self) -> FPoint {
    unsafe {
      FPoint {
        x: (*self.event).x,
        y: (*self.event).y,
      }
    }
  }
}

impl InputEvent for AbsoluteMotionEvent {
  fn raw_device(&self) -> *mut wlr_input_device {
    unsafe { (*self.event).device }
  }

  fn time_msec(&self) -> u32 {
    unsafe { (*self.event).time_msec }
  }
}

impl CursorEvent for AbsoluteMotionEvent {
  fn position(&self) -> FPoint {
    self.cursor_manager.borrow().position()
  }
}

pub struct KeyboardEvent<'a> {
  keyboard: &'a Keyboard,
  event: *const wlr_event_keyboard_key,
}

impl<'a> KeyboardEvent<'a> {
  pub(crate) unsafe fn from_ptr(
    keyboard: &'a Keyboard,
    event: *const wlr_event_keyboard_key,
  ) -> KeyboardEvent {
    KeyboardEvent { keyboard, event }
  }

  pub fn libinput_keycode(&self) -> xkb::Keycode {
    unsafe { (*self.event).keycode }
  }

  pub fn xkb_keycode(&self) -> xkb::Keycode {
    // Translate libinput keycode -> xkbcommon
    unsafe { (*self.event).keycode + 8 }
  }

  pub fn xkb_state(&self) -> &xkb::State {
    self.keyboard.xkb_state()
  }

  pub fn state(&self) -> xkb::StateComponent {
    unsafe { (*self.event).state }
  }

  /// Get the single keysym obtained from pressing a particular key in
  /// a given keyboard state.
  ///
  /// This function is similar to xkb_state_key_get_syms(), but intended
  /// for users which cannot or do not want to handle the case where
  /// multiple keysyms are returned (in which case this function is preferred).
  ///
  /// Returns the keysym. If the key does not have exactly one keysym,
  /// returns xkb::KEY_NoSymbol
  pub fn get_one_sym(&self) -> xkb::Keysym {
    self
      .keyboard
      .xkb_state()
      .key_get_one_sym(self.xkb_keycode())
  }
}

impl<'a> InputEvent for KeyboardEvent<'a> {
  fn raw_device(&self) -> *mut wlr_input_device {
    self.keyboard.device().raw_ptr()
  }

  fn time_msec(&self) -> u32 {
    unsafe { (*self.event).time_msec }
  }
}
