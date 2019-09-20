use crate::geometry::{FDisplacement, FPoint};
use crate::input::keyboard::Keyboard;
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

/// Event that triggers when the pointer device scrolls (e.g using a wheel
/// or in the case of a touchpad when you use two fingers to scroll)
#[derive(Debug)]
pub struct AxisEvent {
  event: *const wlr_event_pointer_axis,
}

impl AxisEvent {
  pub(crate) unsafe fn from_ptr(event: *const wlr_event_pointer_axis) -> Self {
    AxisEvent { event }
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

/// Event that triggers when a button is pressed (e.g left click, right click,
/// a gaming mouse button, etc.)
#[derive(Debug)]
pub struct ButtonEvent {
  event: *const wlr_event_pointer_button,
}

impl ButtonEvent {
  pub(crate) unsafe fn from_ptr(event: *const wlr_event_pointer_button) -> Self {
    ButtonEvent { event }
  }

  /// Get the raw pointer to this event
  pub fn raw_event(&self) -> *const wlr_event_pointer_button {
    self.event
  }

  /// Get the state of the button (e.g pressed or released)
  pub fn state(&self) -> wlr_button_state {
    unsafe { (*self.event).state }
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

/// Event that triggers when the pointer moves
#[derive(Debug)]
pub enum MotionEvent {
  Relative(RelativeMotionEvent),
  Absolute(AbsoluteMotionEvent),
}

impl MotionEvent {
  /// Get the absolute position of the pointer
  pub fn pos(&self) -> FPoint {
    FPoint { x: 0.0, y: 0.0 }
  }
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

#[derive(Debug)]
pub struct RelativeMotionEvent {
  event: *const wlr_event_pointer_motion,
}

impl RelativeMotionEvent {
  pub(crate) unsafe fn from_ptr(event: *const wlr_event_pointer_motion) -> Self {
    RelativeMotionEvent { event }
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

#[derive(Debug)]
pub struct AbsoluteMotionEvent {
  event: *const wlr_event_pointer_motion_absolute,
}

impl AbsoluteMotionEvent {
  pub(crate) unsafe fn from_ptr(event: *const wlr_event_pointer_motion_absolute) -> Self {
    AbsoluteMotionEvent { event }
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

  pub fn state(&self) -> xkb::StateComponent {
    unsafe { (*self.event).state }
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
