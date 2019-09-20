use crate::geometry::{Displacement, Point, Rectangle, Size};
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

  fn parent_offset(&self) -> Displacement {
    match self.surface_type {
      Xdg(xdg_surface) => unsafe {
        if (*xdg_surface).role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_POPUP {
          let popup = &*(*xdg_surface).__bindgen_anon_1.popup;
          let parent = wlr_xdg_surface_from_wlr_surface(popup.parent);
          let mut parent_geo = Rectangle::ZERO.into();
          wlr_xdg_surface_get_geometry(parent, &mut parent_geo);
          Displacement {
            dx: parent_geo.x + popup.geometry.x,
            dy: parent_geo.y + popup.geometry.y,
          }
        } else {
          Displacement::ZERO
        }
      },
      Xwayland(_) => Displacement::ZERO,
    }
  }

  pub fn extents(&self) -> Rectangle {
    unsafe {
      match self.surface_type {
        Xdg(xdg_surface) => {
          let mut wlr_box = Rectangle::ZERO.into();
          wlr_xdg_surface_get_geometry(xdg_surface, &mut wlr_box);
          Rectangle::from(wlr_box) + self.parent_offset()
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

  pub fn buffer_top_left(&self) -> Point {
    unsafe {
      let surface = &*self.surface();
      let top_left = Point {
        x: surface.current.dx,
        y: surface.current.dy,
      };
      top_left + self.parent_offset()
    }
  }

  pub fn buffer_size(&self) -> Size {
    unsafe {
      let surface = &*self.surface();
      Size {
        width: surface.current.width,
        height: surface.current.height,
      }
    }
  }

  pub fn is_inside(&self, point: &Point) -> bool {
    self.extents().contains(&point)
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

pub trait SurfaceEvents {
  fn bind_events<F>(&self, surface_manager: Rc<RefCell<SurfaceManager>>, f: F)
  where
    F: Fn(&mut SurfaceEventManager) -> ();
}

impl SurfaceEvents for Rc<Surface> {
  fn bind_events<F>(&self, surface_manager: Rc<RefCell<SurfaceManager>>, f: F)
  where
    F: Fn(&mut SurfaceEventManager) -> (),
  {
    let event_handler = SurfaceEventHandler {
      surface: Rc::downgrade(self),
      surface_manager: surface_manager.clone(),
    };
    let mut event_manager = SurfaceEventManager::new(event_handler);
    f(&mut event_manager);
    *self.event_manager.borrow_mut() = Some(event_manager);
  }
}

pub struct SurfaceEventHandler {
  surface: Weak<Surface>,
  surface_manager: Rc<RefCell<SurfaceManager>>,
}

impl SurfaceEventHandler {
  fn map(&mut self) {
    if let Some(surface) = self.surface.upgrade() {
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
        .surface_manager
        .borrow_mut()
        .destroy_surface(surface.clone());
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
      .find(|surface| surface.is_inside(point))
      .cloned()
  }

  pub fn new_surface(&mut self, surface_type: SurfaceType) -> Rc<Surface> {
    let surface = Rc::new(Surface {
      surface_type,
      mapped: RefCell::new(false),
      event_manager: RefCell::new(None),
    });
    self.surfaces.push(surface.clone());
    surface
  }

  pub fn destroy_surface(&mut self, destroyed_surface: Rc<Surface>) {
    self
      .surfaces
      .retain(|surface| *surface != destroyed_surface);
  }

  /// Gives keyboard focus to the surface
  pub fn focus_surface(&mut self, surface: Rc<Surface>) {
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
  use crate::test_util::*;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let surface_manager = Rc::new(RefCell::new(SurfaceManager::init(ptr::null_mut())));
    let surface = surface_manager
      .borrow_mut()
      .new_surface(SurfaceType::Xdg(ptr::null_mut()));

    let map_signal = WlSignal::new();
    let unmap_signal = WlSignal::new();
    let destroy_signal = WlSignal::new();

    surface.bind_events(surface_manager.clone(), |event_manager| unsafe {
      event_manager.map(map_signal.ptr());
      event_manager.unmap(unmap_signal.ptr());
      event_manager.destroy(destroy_signal.ptr());
    });

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
