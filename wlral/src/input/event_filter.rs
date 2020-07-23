use crate::input::events::*;
use std::{cell::RefCell, ops::Deref, rc::Rc};
use wlroots_sys::{wlr_backend, wlr_backend_get_session, wlr_session_change_vt};
use xkbcommon::xkb;

/// Implement EventFilter to handle input events.
///
/// Each event handler return a bool to inform if it has handled
/// the event or not. EventFilters are called in order added and
/// as soon as the event is handled, the process stops. If no
/// EventFilter handles the event it will be forwarded to the
/// appropriate client.
pub trait EventFilter {
  fn handle_keyboard_event(&self, _event: &KeyboardEvent) -> bool {
    false
  }
  fn handle_pointer_motion_event(&self, _event: &MotionEvent) -> bool {
    false
  }
  fn handle_pointer_button_event(&self, _event: &ButtonEvent) -> bool {
    false
  }
  fn handle_pointer_axis_event(&self, _event: &AxisEvent) -> bool {
    false
  }
}

impl<T> EventFilter for Rc<T>
where
  T: EventFilter,
{
  fn handle_keyboard_event(&self, event: &KeyboardEvent) -> bool {
    Deref::deref(self).handle_keyboard_event(event)
  }
  fn handle_pointer_motion_event(&self, event: &MotionEvent) -> bool {
    Deref::deref(self).handle_pointer_motion_event(event)
  }
  fn handle_pointer_button_event(&self, event: &ButtonEvent) -> bool {
    Deref::deref(self).handle_pointer_button_event(event)
  }
  fn handle_pointer_axis_event(&self, event: &AxisEvent) -> bool {
    Deref::deref(self).handle_pointer_axis_event(event)
  }
}

pub(crate) struct EventFilterManager {
  event_filters: RefCell<Vec<Box<dyn EventFilter>>>,
}

impl EventFilterManager {
  pub(crate) fn new() -> EventFilterManager {
    EventFilterManager {
      event_filters: RefCell::new(vec![]),
    }
  }

  pub(crate) fn add_event_filter(&self, filter: Box<dyn EventFilter>) {
    self.event_filters.borrow_mut().push(filter)
  }
}

impl EventFilter for EventFilterManager {
  fn handle_keyboard_event(&self, event: &KeyboardEvent) -> bool {
    self
      .event_filters
      .borrow()
      .iter()
      .any(|filter| filter.handle_keyboard_event(event))
  }
  fn handle_pointer_motion_event(&self, event: &MotionEvent) -> bool {
    self
      .event_filters
      .borrow()
      .iter()
      .any(|filter| filter.handle_pointer_motion_event(event))
  }
  fn handle_pointer_button_event(&self, event: &ButtonEvent) -> bool {
    self
      .event_filters
      .borrow()
      .iter()
      .any(|filter| filter.handle_pointer_button_event(event))
  }
  fn handle_pointer_axis_event(&self, event: &AxisEvent) -> bool {
    self
      .event_filters
      .borrow()
      .iter()
      .any(|filter| filter.handle_pointer_axis_event(event))
  }
}

pub struct VtSwitchEventFilter {
  backend: *mut wlr_backend,
}

impl VtSwitchEventFilter {
  pub fn new(backend: *mut wlr_backend) -> VtSwitchEventFilter {
    VtSwitchEventFilter { backend }
  }
}

impl EventFilter for VtSwitchEventFilter {
  fn handle_keyboard_event(&self, event: &KeyboardEvent) -> bool {
    let keysym = event.get_one_sym();
    let vt_range = xkb::KEY_XF86Switch_VT_1..=xkb::KEY_XF86Switch_VT_12;

    if vt_range.contains(&keysym) {
      unsafe {
        let session = wlr_backend_get_session(self.backend);
        if !session.is_null() {
          let vt = keysym - xkb::KEY_XF86Switch_VT_1 + 1;
          wlr_session_change_vt(session, vt);
        }
      }

      true
    } else {
      false
    }
  }
}
