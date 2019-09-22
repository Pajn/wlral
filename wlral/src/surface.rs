use crate::geometry::*;
use crate::shell::xdg::XdgSurface;
use crate::shell::xwayland::XwaylandSurface;
use wlroots_sys::*;

#[derive(PartialEq, Eq)]
pub enum Surface {
  Xdg(XdgSurface),
  Xwayland(XwaylandSurface),
  #[cfg(test)]
  Null,
}

impl Surface {
  pub(crate) fn from_wlr_surface(wlr_surface: *mut wlr_surface) -> Surface {
    if let Ok(xdg_surface) = XdgSurface::from_wlr_surface(wlr_surface) {
      Surface::Xdg(xdg_surface)
    } else if let Ok(xwayland_surface) = XwaylandSurface::from_wlr_surface(wlr_surface) {
      Surface::Xwayland(xwayland_surface)
    } else {
      panic!("Unknown surface type");
    }
  }
}

use Surface::*;

pub(crate) trait SurfaceExt {
  fn wlr_surface(&self) -> *mut wlr_surface;
  fn parent_displacement(&self) -> Displacement;
  fn extents(&self) -> Rectangle;
  fn move_to(&self, top_left: Point);
  fn resize(&self, size: Size);
  fn can_receive_focus(&self) -> bool;
  fn set_activated(&self, activated: bool);
}

impl SurfaceExt for Surface {
  fn wlr_surface(&self) -> *mut wlr_surface {
    match self {
      Xdg(surface) => surface.wlr_surface(),
      Xwayland(surface) => surface.wlr_surface(),
      #[cfg(test)]
      Null => std::ptr::null_mut(),
    }
  }

  fn parent_displacement(&self) -> Displacement {
    match self {
      Xdg(surface) => surface.parent_displacement(),
      Xwayland(surface) => surface.parent_displacement(),
      #[cfg(test)]
      Null => Displacement::ZERO,
    }
  }

  fn extents(&self) -> Rectangle {
    match self {
      Xdg(surface) => surface.extents(),
      Xwayland(surface) => surface.extents(),
      #[cfg(test)]
      Null => Rectangle::ZERO,
    }
  }

  fn move_to(&self, top_left: Point) {
    match self {
      Xdg(surface) => surface.move_to(top_left),
      Xwayland(surface) => surface.move_to(top_left),
      #[cfg(test)]
      Null => {}
    }
  }

  fn resize(&self, size: Size) {
    match self {
      Xdg(surface) => surface.resize(size),
      Xwayland(surface) => surface.resize(size),
      #[cfg(test)]
      Null => {}
    }
  }

  fn can_receive_focus(&self) -> bool {
    match self {
      Xdg(surface) => surface.can_receive_focus(),
      Xwayland(surface) => surface.can_receive_focus(),
      #[cfg(test)]
      Null => false,
    }
  }

  fn set_activated(&self, activated: bool) {
    match self {
      Xdg(surface) => surface.set_activated(activated),
      Xwayland(surface) => surface.set_activated(activated),
      #[cfg(test)]
      Null => {}
    }
  }
}
