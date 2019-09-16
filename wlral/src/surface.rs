use crate::geometry::{Point, Rectangle, Size};
use std::cell::RefCell;
use std::cmp::PartialEq;
use std::pin::Pin;
use std::rc::Rc;
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
  top_left: Point,

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

  pub fn top_left(&self) -> Point {
    self.top_left
  }

  pub fn size(&self) -> Size {
    let surface = self.surface();
    Size {
      width: unsafe { (*surface).current.width },
      height: unsafe { (*surface).current.height },
    }
  }

  pub fn is_inside(&self, point: &Point) -> bool {
    Rectangle {
      top_left: self.top_left(),
      size: self.size(),
    }
    .contains(&point)
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
      surface: self.clone(),
      surface_manager: surface_manager.clone(),
    };
    let mut event_manager = SurfaceEventManager::new(event_handler);
    f(&mut event_manager);
    *self.event_manager.borrow_mut() = Some(event_manager);
  }
}

pub struct SurfaceEventHandler {
  surface: Rc<Surface>,
  surface_manager: Rc<RefCell<SurfaceManager>>,
}

impl SurfaceEventHandler {
  fn map(&mut self) {
    *self.surface.mapped.borrow_mut() = true;
  }

  fn unmap(&mut self) {
    *self.surface.mapped.borrow_mut() = false;
  }

  fn destroy(&mut self) {
    self
      .surface_manager
      .borrow_mut()
      .destroy_surface(self.surface.clone());
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
         handler.destroy()
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
      .find(|surface| surface.is_inside(point))
      .cloned()
  }

  pub fn new_surface(&mut self, surface_type: SurfaceType) -> Rc<Surface> {
    let surface = Rc::new(Surface {
      surface_type,
      mapped: RefCell::new(false),
      top_left: Default::default(),
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
