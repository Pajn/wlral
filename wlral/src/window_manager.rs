use crate::geometry::Point;
use crate::surface::{Surface, SurfaceExt};
use crate::window::Window;
use log::warn;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use wlroots_sys::*;

pub struct WindowManager {
  seat: *mut wlr_seat,
  windows: Vec<Rc<Window>>,
}

impl std::fmt::Debug for WindowManager {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    write!(fmt, "WindowManager {{windows: {0}}}", self.windows.len())
  }
}

impl WindowManager {
  pub fn init(seat: *mut wlr_seat) -> WindowManager {
    WindowManager {
      seat,
      windows: vec![],
    }
  }

  pub fn windows_to_render<'a>(&'a self) -> impl 'a + Iterator<Item = Rc<Window>> {
    self
      .windows
      .iter()
      .filter(|window| *window.mapped.borrow())
      .cloned()
  }

  pub fn window_at(&self, point: &Point) -> Option<Rc<Window>> {
    self
      .windows
      .iter()
      // Reverse as windows is from back to front
      .rev()
      .find(|window| window.extents().contains(point))
      .cloned()
  }

  pub(crate) fn window_buffer_at(&self, point: &Point) -> Option<Rc<Window>> {
    self
      .windows
      .iter()
      // Reverse as windows is from back to front
      .rev()
      .find(|window| window.buffer_extents().contains(point))
      .cloned()
  }

  pub(crate) fn destroy_window(&mut self, destroyed_window: Rc<Window>) {
    self.windows.retain(|window| *window != destroyed_window);
  }

  pub fn windows(&self) -> &Vec<Rc<Window>> {
    &self.windows
  }

  /// Returns the window that holds keyboard focus
  pub fn focused_window(&self) -> Option<Rc<Window>> {
    let focused_surface = unsafe { (*self.seat).keyboard_state.focused_surface };
    self
      .windows
      .iter()
      .find(|w| w.wlr_surface() == focused_surface)
      .cloned()
  }

  /// If the window have keyboard focus
  pub fn window_has_focus(&self, window: &Window) -> bool {
    let wlr_surface = window.wlr_surface();
    let focused_surface = unsafe { (*self.seat).keyboard_state.focused_surface };
    wlr_surface == focused_surface
  }

  /// Gives keyboard focus to the window
  pub fn focus_window(&mut self, window: Rc<Window>) {
    if !window.can_receive_focus() {
      warn!("Window can not receive focus");
      return;
    }
    let wlr_surface = window.wlr_surface();
    unsafe {
      let old_wlr_surface = (*self.seat).keyboard_state.focused_surface;

      if wlr_surface == old_wlr_surface {
        return;
      }

      if !old_wlr_surface.is_null() {
        // Deactivate the previously focused window. This lets the client know
        // it no longer has focus and the client will repaint accordingly, e.g.
        // stop displaying a caret.
        let surface = Surface::from_wlr_surface(old_wlr_surface);
        surface.set_activated(false);
      }

      // Move the view to the front
      self.windows.retain(|s| *s != window);
      self.windows.push(window.clone());

      // Activate the new window
      window.surface().set_activated(true);

      // Tell the seat to have the keyboard enter this window. wlroots will keep
      // track of this and automatically send key events to the appropriate
      // clients without additional work on your part.
      let keyboard = wlr_seat_get_keyboard(self.seat);
      wlr_seat_keyboard_notify_enter(
        self.seat,
        wlr_surface,
        (*keyboard).keycodes.as_mut_ptr(),
        (*keyboard).num_keycodes,
        &mut (*keyboard).modifiers,
      );
    }
  }
}

pub(crate) trait WindowManagerExt {
  fn new_window(&self, surface: Surface) -> Rc<Window>;
}

impl WindowManagerExt for Rc<RefCell<WindowManager>> {
  fn new_window(&self, surface: Surface) -> Rc<Window> {
    let window = Rc::new(Window {
      window_manager: self.clone(),
      surface,
      mapped: RefCell::new(false),
      top_left: RefCell::new(Point::ZERO),
      pending_updates: RefCell::new(BTreeMap::new()),
      event_manager: RefCell::new(None),
    });
    // If the window can receive focus, add it to the back so that
    // the window management policy can choose if it want to focus the
    // window
    if window.can_receive_focus() {
      self.borrow_mut().windows.insert(0, window.clone());
    } else {
      self.borrow_mut().windows.push(window.clone());
    }
    window
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::cursor::MockCursorManager;
  use crate::output_manager::MockOutputManager;
  use crate::window::WindowEventHandler;
  use crate::window_management_policy::WmPolicyManager;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let wm_policy_manager = Rc::new(RefCell::new(WmPolicyManager::new()));
    let cursor_manager = Rc::new(RefCell::new(MockCursorManager::default()));
    let window_manager = Rc::new(RefCell::new(WindowManager::init(ptr::null_mut())));
    let window = window_manager.new_window(Surface::Null);

    let mut event_handler = WindowEventHandler {
      wm_policy_manager,
      output_manager: Rc::new(RefCell::new(MockOutputManager::default())),
      window_manager: window_manager.clone(),
      cursor_manager: cursor_manager.clone(),
      window: Rc::downgrade(&window),
    };

    let weak_window = Rc::downgrade(&window);
    drop(window);

    assert!(weak_window.upgrade().is_some());

    event_handler.destroy();

    assert!(window_manager.borrow().windows.len() == 0);
    assert!(weak_window.upgrade().is_none());
  }
}
