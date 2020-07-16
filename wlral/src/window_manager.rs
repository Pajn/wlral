use crate::geometry::Point;
use crate::surface::{Surface, SurfaceExt};
use crate::{
  event::{Event, EventOnce},
  input::seat::SeatManager,
  output_manager::OutputManager,
  window::Window,
};
use log::warn;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

#[derive(Debug, Copy, Clone)]
pub enum WindowLayer {
  Background,
  Bottom,
  Normal,
  Top,
  Overlay,
}

#[derive(Default)]
struct WindowLayers {
  background: Vec<Rc<Window>>,
  bottom: Vec<Rc<Window>>,
  normal: Vec<Rc<Window>>,
  top: Vec<Rc<Window>>,
  overlay: Vec<Rc<Window>>,
}

impl WindowLayers {
  fn all_windows(&self) -> impl '_ + DoubleEndedIterator<Item = Rc<Window>> {
    self
      .background
      .iter()
      .chain(self.bottom.iter())
      .chain(self.normal.iter())
      .chain(self.top.iter())
      .chain(self.overlay.iter())
      .cloned()
  }

  fn update<F>(&mut self, layer: WindowLayer, mut f: F)
  where
    F: FnMut(&mut Vec<Rc<Window>>) -> (),
  {
    match layer {
      WindowLayer::Background => f(&mut self.background),
      WindowLayer::Bottom => f(&mut self.bottom),
      WindowLayer::Normal => f(&mut self.normal),
      WindowLayer::Top => f(&mut self.top),
      WindowLayer::Overlay => f(&mut self.overlay),
    }
  }
}

pub struct WindowManager {
  seat_manager: Rc<SeatManager>,
  output_manager: RefCell<Weak<OutputManager>>,
  layers: RefCell<WindowLayers>,
  foreign_toplevel_manager: *mut wlr_foreign_toplevel_manager_v1,
}

impl std::fmt::Debug for WindowManager {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    write!(
      fmt,
      "WindowManager {{windows: {0}}}",
      self.layers.borrow().normal.len()
    )
  }
}

impl WindowManager {
  pub(crate) fn init(seat_manager: Rc<SeatManager>, display: *mut wl_display) -> WindowManager {
    let foreign_toplevel_manager = unsafe { wlr_foreign_toplevel_manager_v1_create(display) };
    WindowManager {
      seat_manager,
      output_manager: RefCell::new(Weak::<OutputManager>::new()),
      layers: RefCell::new(WindowLayers::default()),
      foreign_toplevel_manager,
    }
  }

  pub fn raw_foreign_toplevel_manager(&self) -> *mut wlr_foreign_toplevel_manager_v1 {
    self.foreign_toplevel_manager
  }

  pub fn windows_to_render(&self) -> impl '_ + Iterator<Item = Rc<Window>> {
    self.windows().filter(|window| *window.mapped.borrow())
  }

  pub fn window_at(&self, point: &Point) -> Option<Rc<Window>> {
    self
      .layers
      .borrow()
      .all_windows()
      // Reverse as windows is from back to front
      .rev()
      .find(|window| window.extents().contains(point))
  }

  pub(crate) fn window_buffer_at(&self, point: &Point) -> Option<Rc<Window>> {
    self
      .layers
      .borrow()
      .all_windows()
      // Reverse as windows is from back to front
      .rev()
      .find(|window| window.buffer_extents().contains(point))
  }

  pub(crate) fn destroy_window(&self, destroyed_window: Rc<Window>) {
    self
      .layers
      .borrow_mut()
      .update(destroyed_window.layer, |windows| {
        windows.retain(|window| *window != destroyed_window)
      });
  }

  pub fn windows(&self) -> impl '_ + DoubleEndedIterator<Item = Rc<Window>> {
    let windows = self.layers.borrow().all_windows().collect::<Vec<_>>();
    windows.into_iter()
  }

  /// Returns the window that holds keyboard focus
  pub fn focused_window(&self) -> Option<Rc<Window>> {
    let focused_surface = unsafe {
      (*self.seat_manager.raw_seat())
        .keyboard_state
        .focused_surface
    };
    self
      .layers
      .borrow()
      .all_windows()
      .find(|w| w.wlr_surface() == focused_surface)
  }

  /// If the window have keyboard focus
  pub fn window_has_focus(&self, window: &Window) -> bool {
    let wlr_surface = window.wlr_surface();
    let focused_surface = unsafe {
      (*self.seat_manager.raw_seat())
        .keyboard_state
        .focused_surface
    };
    wlr_surface == focused_surface
  }

  /// Gives keyboard focus to the window
  pub fn focus_window(&self, window: Rc<Window>) {
    if !window.can_receive_focus() {
      warn!("Window can not receive focus");
      return;
    }
    if !self.seat_manager.is_input_allowed(&window) {
      warn!("Refusing to set focus, input is inhibited");
      return;
    }
    let wlr_surface = window.wlr_surface();
    unsafe {
      let old_wlr_surface = (*self.seat_manager.raw_seat())
        .keyboard_state
        .focused_surface;

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
      self.layers.borrow_mut().update(window.layer, |windows| {
        windows.retain(|s| *s != window);
        windows.push(window.clone());
      });

      // Activate the new window
      window.surface().set_activated(true);

      // Tell the seat to have the keyboard enter this window. wlroots will keep
      // track of this and automatically send key events to the appropriate
      // clients without additional work on your part.
      let keyboard = wlr_seat_get_keyboard(self.seat_manager.raw_seat());
      wlr_seat_keyboard_notify_enter(
        self.seat_manager.raw_seat(),
        wlr_surface,
        (*keyboard).keycodes.as_mut_ptr(),
        (*keyboard).num_keycodes,
        &mut (*keyboard).modifiers,
      );
    }
  }

  /// Blurs the currently focused window without focusing another one
  pub fn blur(&self) {
    unsafe {
      let old_wlr_surface = (*self.seat_manager.raw_seat())
        .keyboard_state
        .focused_surface;
      if !old_wlr_surface.is_null() {
        // Deactivate the previously focused window. This lets the client know
        // it no longer has focus and the client will repaint accordingly, e.g.
        // stop displaying a caret.
        let surface = Surface::from_wlr_surface(old_wlr_surface);
        surface.set_activated(false);
      }

      wlr_seat_keyboard_clear_focus(self.seat_manager.raw_seat());
    }
  }
}

pub(crate) trait WindowManagerExt {
  fn set_output_manager(&self, output_manager: Rc<OutputManager>);
  fn new_window(&self, layer: WindowLayer, surface: Surface) -> Rc<Window>;
}

impl WindowManagerExt for Rc<WindowManager> {
  fn set_output_manager(&self, output_manager: Rc<OutputManager>) {
    *self.output_manager.borrow_mut() = Rc::downgrade(&output_manager);
    let window_manager = self.clone();
    output_manager
      .on_output_layout_change()
      .subscribe(Box::new(move |_| {
        for window in window_manager.layers.borrow().all_windows() {
          window.update_outputs();
        }
      }));
  }

  fn new_window(&self, layer: WindowLayer, surface: Surface) -> Rc<Window> {
    let window = Rc::new(Window {
      output_manager: self.output_manager.borrow().upgrade().expect("window_manager should be initialized with and output_manager before windows can be created"),
      window_manager: self.clone(),
      layer,
      surface,
      mapped: RefCell::new(false),
      top_left: RefCell::new(Point::ZERO),
      outputs: RefCell::new(vec![]),
      minimize_targets: RefCell::new(vec![]),
      pending_updates: RefCell::new(BTreeMap::new()),
      on_entered_output: Event::default(),
      on_left_output: Event::default(),
      on_destroy: EventOnce::default(),
      event_manager: RefCell::new(None),
    });
    // If the window can receive focus, add it to the back so that
    // the window management policy can choose if it want to focus the
    // window
    if window.can_receive_focus() {
      self.layers.borrow_mut().update(layer, |windows| {
        windows.insert(0, window.clone());
      })
    } else {
      self.layers.borrow_mut().update(layer, |windows| {
        windows.push(window.clone());
      })
    }
    window
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::{cursor::CursorManager, event_filter::EventFilterManager};
  use crate::output_manager::OutputManager;
  use crate::window::WindowEventHandler;
  use crate::window_management_policy::WmPolicyManager;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let wm_policy_manager = Rc::new(RefCell::new(WmPolicyManager::new()));
    let seat_manager = SeatManager::mock(ptr::null_mut(), ptr::null_mut());
    let window_manager = Rc::new(WindowManager::init(seat_manager.clone(), ptr::null_mut()));
    let output_manager = OutputManager::mock(wm_policy_manager.clone(), window_manager.clone());
    let cursor_manager = CursorManager::mock(
      output_manager.clone(),
      window_manager.clone(),
      seat_manager.clone(),
      Rc::new(RefCell::new(EventFilterManager::new())),
      ptr::null_mut(),
      ptr::null_mut(),
    );

    window_manager.set_output_manager(output_manager.clone());
    let window = window_manager.new_window(WindowLayer::Normal, Surface::Null);

    let mut event_handler = WindowEventHandler {
      wm_policy_manager,
      output_manager: output_manager.clone(),
      window_manager: window_manager.clone(),
      cursor_manager: cursor_manager.clone(),
      window: Rc::downgrade(&window),
      foreign_toplevel_handle: None,
      foreign_toplevel_event_manager: None,
    };

    let weak_window = Rc::downgrade(&window);
    drop(window);

    assert!(window_manager.windows().count() == 1);
    assert!(weak_window.upgrade().is_some());

    event_handler.destroy();

    assert!(window_manager.windows().count() == 0);
    assert!(weak_window.upgrade().is_none());
  }
}

#[cfg(test)]
unsafe fn wlr_foreign_toplevel_manager_v1_create(
  _display: *mut wl_display,
) -> *mut wlr_foreign_toplevel_manager_v1 {
  std::ptr::null_mut()
}
