use crate::geometry::*;
use crate::shell::xdg::{XdgSurface, XdgSurfaceEventManager};
use crate::shell::xwayland::{XwaylandSurface, XwaylandSurfaceEventManager};
use std::pin::Pin;
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
  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface>;
  fn buffer_displacement(&self) -> Displacement;
  fn parent_displacement(&self) -> Displacement;

  fn extents(&self) -> Rectangle;
  fn move_to(&self, top_left: Point);
  fn resize(&self, size: Size);

  fn can_receive_focus(&self) -> bool;
  fn activated(&self) -> bool;
  fn set_activated(&self, activated: bool);

  fn maximized(&self) -> bool;
  fn set_maximized(&self, maximized: bool);
  fn fullscreen(&self) -> bool;
  fn set_fullscreen(&self, fullscreen: bool);
  fn resizing(&self) -> bool;
  fn set_resizing(&self, resizing: bool);

  fn ask_client_to_close(&self);
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

  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    match self {
      Xdg(surface) => surface.parent_wlr_surface(),
      Xwayland(surface) => surface.parent_wlr_surface(),
      #[cfg(test)]
      Null => None,
    }
  }

  fn buffer_displacement(&self) -> Displacement {
    match self {
      Xdg(surface) => surface.buffer_displacement(),
      Xwayland(surface) => surface.buffer_displacement(),
      #[cfg(test)]
      Null => Displacement::ZERO,
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
  fn activated(&self) -> bool {
    match self {
      Xdg(surface) => surface.activated(),
      Xwayland(surface) => surface.activated(),
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

  fn maximized(&self) -> bool {
    match self {
      Xdg(surface) => surface.maximized(),
      Xwayland(surface) => surface.maximized(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_maximized(&self, maximized: bool) {
    match self {
      Xdg(surface) => surface.set_maximized(maximized),
      Xwayland(surface) => surface.set_maximized(maximized),
      #[cfg(test)]
      Null => {}
    }
  }
  fn fullscreen(&self) -> bool {
    match self {
      Xdg(surface) => surface.fullscreen(),
      Xwayland(surface) => surface.fullscreen(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_fullscreen(&self, fullscreen: bool) {
    match self {
      Xdg(surface) => surface.set_fullscreen(fullscreen),
      Xwayland(surface) => surface.set_fullscreen(fullscreen),
      #[cfg(test)]
      Null => {}
    }
  }
  fn resizing(&self) -> bool {
    match self {
      Xdg(surface) => surface.resizing(),
      Xwayland(surface) => surface.resizing(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_resizing(&self, resizing: bool) {
    match self {
      Xdg(surface) => surface.set_resizing(resizing),
      Xwayland(surface) => surface.set_resizing(resizing),
      #[cfg(test)]
      Null => {}
    }
  }

  fn ask_client_to_close(&self) {
    match self {
      Xdg(surface) => surface.ask_client_to_close(),
      Xwayland(surface) => surface.ask_client_to_close(),
      #[cfg(test)]
      Null => {}
    }
  }
}

pub enum SurfaceEventManager {
  Xdg(Pin<Box<XdgSurfaceEventManager>>),
  Xwayland(Pin<Box<XwaylandSurfaceEventManager>>),
}
