use crate::geometry::{Displacement, FPoint, Point, Rectangle, Size};
use crate::input::cursor::CursorManager;
use crate::window_management_policy::*;
use std::cell::RefCell;
use std::cmp::PartialEq;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use wlroots_sys::*;

#[derive(Debug)]
pub enum SurfaceType {
  Xdg(*mut wlr_xdg_surface),
  Xwayland(*mut wlr_xwayland_surface),
}
use SurfaceType::*;

pub struct Surface {
  surface_type: SurfaceType,
  mapped: RefCell<bool>,
  top_left: RefCell<Point>,

  event_manager: RefCell<Option<Pin<Box<SurfaceEventManager>>>>,
}

impl Surface {
  pub fn surface(&self) -> *mut wlr_surface {
    unsafe {
      match self.surface_type {
        Xdg(xdg_surface) => (*xdg_surface).surface,
        Xwayland(xwayland_surface) => (*xwayland_surface).surface,
      }
    }
  }

  fn top_left(&self) -> Displacement {
    match self.surface_type {
      Xdg(xdg_surface) => unsafe {
        if (*xdg_surface).role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_POPUP {
          let popup = &*(*xdg_surface).__bindgen_anon_1.popup;
          let parent = wlr_xdg_surface_from_wlr_surface(popup.parent);
          let mut parent_geo = Rectangle::ZERO.into();

          wlr_xdg_surface_get_geometry(parent, &mut parent_geo);

          self.top_left.borrow().as_displacement()
            + Displacement {
              dx: parent_geo.x + popup.geometry.x,
              dy: parent_geo.y + popup.geometry.y,
            }
        } else {
          self.top_left.borrow().as_displacement()
        }
      },
      Xwayland(_) => self.top_left.borrow().as_displacement(),
    }
  }

  /// The position and size of the window
  pub fn extents(&self) -> Rectangle {
    unsafe {
      match self.surface_type {
        Xdg(xdg_surface) => {
          let mut wlr_box = Rectangle::ZERO.into();
          wlr_xdg_surface_get_geometry(xdg_surface, &mut wlr_box);
          Rectangle::from(wlr_box) + self.top_left()
        }
        Xwayland(xwayland_surface) => Rectangle {
          top_left: Point {
            x: (*xwayland_surface).x as i32,
            y: (*xwayland_surface).y as i32,
          },
          size: Size {
            width: (*xwayland_surface).width as i32,
            height: (*xwayland_surface).height as i32,
          },
        },
      }
    }
  }

  /// The position and size of the buffer
  ///
  /// When a client draws client-side shadows (liek GTK)
  /// this is larger than the window extents to also fit
  /// said shadows.
  pub fn buffer_extents(&self) -> Rectangle {
    unsafe {
      let surface = &*self.surface();

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

    if let Xwayland(xwayland_surface) = self.surface_type {
      unsafe {
        wlr_xwayland_surface_configure(
          xwayland_surface,
          top_left.x as i16,
          top_left.y as i16,
          (*xwayland_surface).width,
          (*xwayland_surface).height,
        );
      }
    }
  }

  pub fn resize(&self, size: Size) {
    match self.surface_type {
      Xdg(xdg_surface) => unsafe {
        if (*xdg_surface).role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_TOPLEVEL {
          wlr_xdg_toplevel_set_size(xdg_surface, size.width as u32, size.height as u32);
        }
      },
      Xwayland(xwayland_surface) => unsafe {
        wlr_xwayland_surface_configure(
          xwayland_surface,
          (*xwayland_surface).x,
          (*xwayland_surface).y,
          size.width as u16,
          size.height as u16,
        );
      },
    }
  }

  pub fn can_receive_focus(&self) -> bool {
    match self.surface_type {
      Xdg(xdg_surface) => unsafe {
        (*xdg_surface).role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_TOPLEVEL
      },
      // TODO: Is this true?
      Xwayland(_) => true,
    }
  }
}

impl PartialEq for Surface {
  fn eq(&self, other: &Surface) -> bool {
    match (&self.surface_type, &other.surface_type) {
      (Xdg(self_surface), Xdg(other_surface)) => self_surface == other_surface,
      (Xwayland(self_surface), Xwayland(other_surface)) => self_surface == other_surface,
      _ => false,
    }
  }
}

pub(crate) trait SurfaceEvents {
  fn bind_events<F>(
    &self,
    wm_manager: Rc<RefCell<WmManager>>,
    surface_manager: Rc<RefCell<SurfaceManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    f: F,
  ) where
    F: Fn(&mut SurfaceEventManager) -> ();
}

impl SurfaceEvents for Rc<Surface> {
  fn bind_events<F>(
    &self,
    wm_manager: Rc<RefCell<WmManager>>,
    surface_manager: Rc<RefCell<SurfaceManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    f: F,
  ) where
    F: Fn(&mut SurfaceEventManager) -> (),
  {
    let event_handler = SurfaceEventHandler {
      wm_manager,
      surface_manager,
      cursor_manager,
      surface: Rc::downgrade(self),
    };
    let mut event_manager = SurfaceEventManager::new(event_handler);
    f(&mut event_manager);
    *self.event_manager.borrow_mut() = Some(event_manager);
  }
}

pub struct SurfaceEventHandler {
  wm_manager: Rc<RefCell<WmManager>>,
  surface_manager: Rc<RefCell<SurfaceManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
  surface: Weak<Surface>,
}

impl SurfaceEventHandler {
  fn map(&mut self) {
    if let Some(surface) = self.surface.upgrade() {
      self
        .wm_manager
        .borrow_mut()
        .handle_window_ready(surface.clone());
      *surface.mapped.borrow_mut() = true;
    }
  }

  fn unmap(&mut self) {
    if let Some(surface) = self.surface.upgrade() {
      *surface.mapped.borrow_mut() = false;
    }
  }

  fn destroy(&mut self) {
    if let Some(surface) = self.surface.upgrade() {
      self
        .wm_manager
        .borrow_mut()
        .advise_delete_window(surface.clone());
      self
        .surface_manager
        .borrow_mut()
        .destroy_surface(surface.clone());
    }
  }

  fn request_move(&mut self) {
    if let Some(surface) = self.surface.upgrade() {
      let event = MoveEvent {
        surface: surface.clone(),
        drag_point: self.cursor_manager.borrow().position()
          - FPoint::from(surface.extents().top_left()).as_displacement(),
      };

      self.wm_manager.borrow_mut().handle_request_move(event);
    }
  }

  fn request_resize(&mut self, event: *mut wlr_xdg_toplevel_resize_event) {
    if let Some(surface) = self.surface.upgrade() {
      let event = ResizeEvent {
        surface: surface.clone(),
        cursor_position: self.cursor_manager.borrow().position(),
        edges: WindowEdge::from_bits_truncate(unsafe { (*event).edges }),
      };

      self.wm_manager.borrow_mut().handle_request_resize(event);
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

pub struct SurfaceManager {
  seat: *mut wlr_seat,
  surfaces: Vec<Rc<Surface>>,
}

impl SurfaceManager {
  pub fn init(seat: *mut wlr_seat) -> SurfaceManager {
    SurfaceManager {
      seat,
      surfaces: vec![],
    }
  }

  pub fn surfaces_to_render<'a>(&'a self) -> impl 'a + Iterator<Item = Rc<Surface>> {
    self
      .surfaces
      .iter()
      .filter(|surface| *surface.mapped.borrow())
      .cloned()
  }

  pub fn surface_at(&self, point: &Point) -> Option<Rc<Surface>> {
    self
      .surfaces
      .iter()
      // Reverse as surfaces is from back to front
      .rev()
      .find(|surface| surface.extents().contains(point))
      .cloned()
  }

  pub(crate) fn surface_buffer_at(&self, point: &Point) -> Option<Rc<Surface>> {
    self
      .surfaces
      .iter()
      // Reverse as surfaces is from back to front
      .rev()
      .find(|surface| surface.buffer_extents().contains(point))
      .cloned()
  }

  pub fn new_surface(&mut self, surface_type: SurfaceType) -> Rc<Surface> {
    let surface = Rc::new(Surface {
      surface_type,
      mapped: RefCell::new(false),
      top_left: RefCell::new(Point::ZERO),
      event_manager: RefCell::new(None),
    });
    self.surfaces.insert(0, surface.clone());
    surface
  }

  pub fn destroy_surface(&mut self, destroyed_surface: Rc<Surface>) {
    self
      .surfaces
      .retain(|surface| *surface != destroyed_surface);
  }

  /// If the window have keyboard focus
  pub fn surface_has_focus(&self, surface: &Surface) -> bool {
    let surface_ptr = surface.surface();
    let focused_surface = unsafe { (*self.seat).keyboard_state.focused_surface };
    surface_ptr == focused_surface
  }

  /// Gives keyboard focus to the surface
  pub fn focus_surface(&mut self, surface: Rc<Surface>) {
    if !surface.can_receive_focus() {
      eprintln!("Surface can not receive focus");
      return;
    }
    let surface_ptr = surface.surface();
    unsafe {
      let old_surface = (*self.seat).keyboard_state.focused_surface;

      if surface_ptr == old_surface {
        return;
      }

      if !old_surface.is_null() {
        // Deactivate the previously focused surface. This lets the client know
        // it no longer has focus and the client will repaint accordingly, e.g.
        // stop displaying a caret.
        if wlr_surface_is_xdg_surface(old_surface) {
          let xdg_surface = wlr_xdg_surface_from_wlr_surface(old_surface);
          wlr_xdg_toplevel_set_activated(xdg_surface, false);
        } else if wlr_surface_is_xwayland_surface(old_surface) {
          let xwayland_surface = wlr_xwayland_surface_from_wlr_surface(old_surface);
          wlr_xwayland_surface_activate(xwayland_surface, true);
        } else {
          eprintln!("Unknown old surface type");
        }
      }

      // Move the view to the front
      self.surfaces.retain(|s| *s != surface);
      self.surfaces.push(surface.clone());

      // Activate the new surface
      match surface.surface_type {
        Xdg(xdg_surface) => {
          wlr_xdg_toplevel_set_activated(xdg_surface, true);
        }
        Xwayland(xwayland_surface) => {
          wlr_xwayland_surface_activate(xwayland_surface, true);
        }
      }

      // Tell the seat to have the keyboard enter this surface. wlroots will keep
      // track of this and automatically send key events to the appropriate
      // clients without additional work on your part.
      let keyboard = wlr_seat_get_keyboard(self.seat);
      wlr_seat_keyboard_notify_enter(
        self.seat,
        surface_ptr,
        (*keyboard).keycodes.as_mut_ptr(),
        (*keyboard).num_keycodes,
        &mut (*keyboard).modifiers,
      );
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::cursor::MockCursorManager;
  use crate::test_util::*;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let wm_manager = Rc::new(RefCell::new(WmManager::new()));
    let cursor_manager = Rc::new(RefCell::new(MockCursorManager::default()));
    let surface_manager = Rc::new(RefCell::new(SurfaceManager::init(ptr::null_mut())));
    let surface = surface_manager
      .borrow_mut()
      .new_surface(SurfaceType::Xdg(ptr::null_mut()));

    let map_signal = WlSignal::new();
    let unmap_signal = WlSignal::new();
    let destroy_signal = WlSignal::new();

    surface.bind_events(
      wm_manager,
      surface_manager.clone(),
      cursor_manager.clone(),
      |event_manager| unsafe {
        event_manager.map(map_signal.ptr());
        event_manager.unmap(unmap_signal.ptr());
        event_manager.destroy(destroy_signal.ptr());
      },
    );

    let weak_surface = Rc::downgrade(&surface);
    drop(surface);

    assert!(weak_surface.upgrade().is_some());
    assert!(destroy_signal.listener_count() == 1);

    destroy_signal.emit();

    assert!(destroy_signal.listener_count() == 0);
    assert!(surface_manager.borrow().surfaces.len() == 0);
    assert!(weak_surface.upgrade().is_none());
  }
}
