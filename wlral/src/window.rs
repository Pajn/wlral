use crate::geometry::{Displacement, FPoint, Point, Rectangle, Size};
use crate::input::cursor::CursorManager;
use crate::surface::{Surface, SurfaceExt};
use crate::window_management_policy::*;
use crate::window_manager::WindowManager;
use std::cell::RefCell;
use std::cmp::PartialEq;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

pub struct Window {
  pub(crate) surface: Surface,
  pub(crate) mapped: RefCell<bool>,
  pub(crate) top_left: RefCell<Point>,

  pub(crate) event_manager: RefCell<Option<Pin<Box<SurfaceEventManager>>>>,
}

impl Window {
  pub(crate) fn surface(&self) -> &Surface {
    &self.surface
  }

  pub fn wlr_surface(&self) -> *mut wlr_surface {
    self.surface.wlr_surface()
  }

  fn top_left(&self) -> Displacement {
    self.top_left.borrow().as_displacement() + self.surface.parent_displacement()
  }

  /// The position and size of the window
  pub fn extents(&self) -> Rectangle {
    self.surface.extents() + self.top_left()
  }

  /// The position and size of the buffer
  ///
  /// When a client draws client-side shadows (liek GTK)
  /// this is larger than the window extents to also fit
  /// said shadows.
  pub fn buffer_extents(&self) -> Rectangle {
    unsafe {
      let surface = &*self.wlr_surface();

      Rectangle {
        top_left: Point {
          x: surface.current.dx,
          y: surface.current.dy,
        },
        size: Size {
          width: surface.current.width,
          height: surface.current.height,
        },
      } + self.top_left()
    }
  }

  pub fn move_to(&self, top_left: Point) {
    let buffer_displacement = self.extents().top_left() - self.buffer_extents().top_left();
    *self.top_left.borrow_mut() = top_left - buffer_displacement;

    self.surface.move_to(top_left)
  }

  pub fn resize(&self, size: Size) {
    self.surface.resize(size)
  }

  pub fn can_receive_focus(&self) -> bool {
    self.surface.can_receive_focus()
  }
}

impl PartialEq for Window {
  fn eq(&self, other: &Window) -> bool {
    self.surface == other.surface
  }
}

pub(crate) trait WindowEvents {
  fn bind_events<F>(
    &self,
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    f: F,
  ) where
    F: Fn(&mut SurfaceEventManager) -> ();
}

impl WindowEvents for Rc<Window> {
  fn bind_events<F>(
    &self,
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    f: F,
  ) where
    F: Fn(&mut SurfaceEventManager) -> (),
  {
    let event_handler = SurfaceEventHandler {
      wm_policy_manager,
      window_manager,
      cursor_manager,
      window: Rc::downgrade(self),
    };
    let mut event_manager = SurfaceEventManager::new(event_handler);
    f(&mut event_manager);
    *self.event_manager.borrow_mut() = Some(event_manager);
  }
}

pub struct SurfaceEventHandler {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  window: Weak<Window>,
}

impl SurfaceEventHandler {
  fn map(&mut self) {
    if let Some(window) = self.window.upgrade() {
      self
        .wm_policy_manager
        .borrow_mut()
        .handle_window_ready(window.clone());
      *window.mapped.borrow_mut() = true;
    }
  }

  fn unmap(&mut self) {
    if let Some(window) = self.window.upgrade() {
      *window.mapped.borrow_mut() = false;
    }
  }

  fn destroy(&mut self) {
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

  fn request_move(&mut self) {
    if let Some(window) = self.window.upgrade() {
      let event = MoveEvent {
        window: window.clone(),
        drag_point: self.cursor_manager.borrow().position()
          - FPoint::from(window.extents().top_left()).as_displacement(),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_move(event);
    }
  }

  fn request_resize(&mut self, event: *mut wlr_xdg_toplevel_resize_event) {
    if let Some(window) = self.window.upgrade() {
      let event = ResizeEvent {
        window: window.clone(),
        cursor_position: self.cursor_manager.borrow().position(),
        edges: WindowEdge::from_bits_truncate(unsafe { (*event).edges }),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_resize(event);
    }
  }
}

wayland_listener!(
  pub SurfaceEventManager,
  SurfaceEventHandler,
  [
    map => map_func: |this: &mut SurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.map()
    };
    unmap => unmap_func: |this: &mut SurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.unmap()
    };
    destroy => destroy_func: |this: &mut SurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.destroy();
    };
    request_move => request_move_func: |this: &mut SurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_move();
    };
    request_resize => request_resize_func: |this: &mut SurfaceEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_resize(data as _);
    };
  ]
);
