use crate::geometry::{Displacement, FPoint, Point, Rectangle, Size};
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window_management_policy::*;
use crate::window_manager::WindowManager;
use bitflags::bitflags;
use std::cell::RefCell;
use std::cmp::PartialEq;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

bitflags! {
  pub struct WindowEdge: u32 {
    const NONE   = 0b0000;
    const TOP    = 0b0001;
    const BOTTOM = 0b0010;
    const LEFT   = 0b0100;
    const RIGHT  = 0b1000;
  }
}

#[derive(Debug)]
pub struct PendingUpdate {
  top_left: Point,
}

#[derive(Debug)]
pub struct Window {
  pub(crate) window_manager: Rc<RefCell<WindowManager>>,

  pub(crate) surface: Surface,
  pub(crate) mapped: RefCell<bool>,
  pub(crate) top_left: RefCell<Point>,

  pub(crate) pending_updates: RefCell<BTreeMap<u32, PendingUpdate>>,

  pub(crate) event_manager: RefCell<Option<SurfaceEventManager>>,
}

impl Window {
  pub(crate) fn surface(&self) -> &Surface {
    &self.surface
  }

  pub fn wlr_surface(&self) -> *mut wlr_surface {
    self.surface.wlr_surface()
  }

  fn position_displacement(&self) -> Displacement {
    let parent_displacement = self
      .surface
      .parent_wlr_surface()
      .and_then(|parent_wlr_surface| {
        self
          .window_manager
          .borrow()
          .windows()
          .iter()
          .find(|w| w.wlr_surface() == parent_wlr_surface)
          .cloned()
      })
      .map(|w| w.buffer_extents().top_left().as_displacement())
      .unwrap_or_default();

    self.top_left.borrow().as_displacement()
      + parent_displacement
      + self.surface.parent_displacement()
      - self.surface.buffer_displacement()
  }

  /// The position and size of the window
  pub fn extents(&self) -> Rectangle {
    self.surface.extents() + self.position_displacement()
  }

  /// The position and size of the buffer
  ///
  /// When a client draws client-side shadows (like GTK)
  /// this is larger than the window extents to also fit
  /// said shadows.
  pub fn buffer_extents(&self) -> Rectangle {
    let surface = unsafe { &*self.wlr_surface() };

    let buffer_rect = Rectangle {
      top_left: Point {
        x: surface.current.dx,
        y: surface.current.dy,
      },
      size: Size {
        width: surface.current.width,
        height: surface.current.height,
      },
    };

    buffer_rect + self.position_displacement()
  }

  /// Atomically updates position and size
  ///
  /// As size updates have to be communicated to the client,
  /// this will not cause an immediately observable effect.
  pub fn set_extents(&self, extents: &Rectangle) {
    self.pending_updates.borrow_mut().insert(
      self.surface.resize(extents.size),
      PendingUpdate {
        top_left: extents.top_left(),
      },
    );
  }

  pub fn move_to(&self, top_left: Point) {
    *self.top_left.borrow_mut() = top_left;

    self.surface.move_to(top_left);
  }

  pub fn resize(&self, size: Size) {
    self.surface.resize(size);
  }

  pub fn activated(&self) -> bool {
    self.surface.activated()
  }
  pub fn can_receive_focus(&self) -> bool {
    self.surface.can_receive_focus()
  }
  pub fn set_activated(&self, activated: bool) {
    self.surface.set_activated(activated);
  }

  pub fn maximized(&self) -> bool {
    self.surface.maximized()
  }
  pub fn set_maximized(&self, maximized: bool) {
    self.surface.set_maximized(maximized);
  }
  pub fn fullscreen(&self) -> bool {
    self.surface.fullscreen()
  }
  pub fn set_fullscreen(&self, fullscreen: bool) {
    self.surface.set_fullscreen(fullscreen);
  }
  pub fn resizing(&self) -> bool {
    self.surface.resizing()
  }
  pub fn set_resizing(&self, resizing: bool) {
    self.surface.set_resizing(resizing);
  }

  pub fn app_id(&self) -> Option<String> {
    self.surface.app_id()
  }
  pub fn title(&self) -> Option<String> {
    self.surface.title()
  }

  pub fn ask_client_to_close(&self) {
    self.surface.ask_client_to_close()
  }
}

impl PartialEq for Window {
  fn eq(&self, other: &Window) -> bool {
    self.surface == other.surface
  }
}

pub(crate) struct WindowCommitEvent {
  pub(crate) serial: u32,
}

pub(crate) struct WindowResizeEvent {
  pub(crate) edges: u32,
}

pub(crate) struct WindowMaximizeEvent {
  pub(crate) maximize: bool,
}

pub(crate) struct WindowFullscreenEvent {
  pub(crate) fullscreen: bool,
  pub(crate) output: Option<*mut wlr_output>,
}

pub(crate) struct WindowEventHandler {
  pub(crate) wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  pub(crate) output_manager: Rc<RefCell<dyn OutputManager>>,
  pub(crate) window_manager: Rc<RefCell<WindowManager>>,
  pub(crate) cursor_manager: Rc<RefCell<dyn CursorManager>>,
  pub(crate) window: Weak<Window>,
}

impl WindowEventHandler {
  pub(crate) fn map(&mut self) {
    if let Some(window) = self.window.upgrade() {
      self
        .wm_policy_manager
        .borrow_mut()
        .handle_window_ready(window.clone());
      *window.mapped.borrow_mut() = true;
    }
  }

  pub(crate) fn unmap(&mut self) {
    if let Some(window) = self.window.upgrade() {
      *window.mapped.borrow_mut() = false;
    }
  }

  pub(crate) fn destroy(&mut self) {
    if let Some(window) = self.window.upgrade() {
      self
        .wm_policy_manager
        .borrow_mut()
        .advise_delete_window(window.clone());
      self
        .window_manager
        .borrow_mut()
        .destroy_window(window.clone());
    }
  }

  pub(crate) fn commit(&mut self, event: WindowCommitEvent) {
    if let Some(window) = self.window.upgrade() {
      match window.pending_updates.borrow_mut().remove(&event.serial) {
        Some(update) => {
          window.move_to(update.top_left);
        }
        _ => {}
      }
      self
        .wm_policy_manager
        .borrow_mut()
        .advise_configured_window(window.clone());
    }
  }

  pub(crate) fn request_move(&mut self) {
    if let Some(window) = self.window.upgrade() {
      let request = MoveRequest {
        window: window.clone(),
        drag_point: self.cursor_manager.borrow().position()
          - FPoint::from(window.extents().top_left()).as_displacement(),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_move(request);
    }
  }

  pub(crate) fn request_resize(&mut self, event: WindowResizeEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = ResizeRequest {
        window: window.clone(),
        cursor_position: self.cursor_manager.borrow().position(),
        edges: WindowEdge::from_bits_truncate(event.edges),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_resize(request);
    }
  }

  pub(crate) fn request_maximize(&mut self, event: WindowMaximizeEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = MaximizeRequest {
        window: window.clone(),
        maximize: event.maximize,
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_maximize(request);
    }
  }
  pub(crate) fn request_fullscreen(&mut self, event: WindowFullscreenEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = FullscreenRequest {
        window: window.clone(),
        fullscreen: event.fullscreen,
        output: self
          .output_manager
          .borrow()
          .outputs()
          .iter()
          .find(|o| Some(o.raw_ptr()) == event.output)
          .cloned(),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_fullscreen(request);
    }
  }
  pub(crate) fn request_minimize(&mut self) {
    if let Some(window) = self.window.upgrade() {
      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_minimize(window.clone());
    }
  }
}
