use crate::geometry::*;
use crate::shell::layer::{LayerSurface, LayerSurfaceEventManager};
use crate::shell::xdg::{XdgSurface, XdgSurfaceEventManager};
use crate::shell::xwayland::{XwaylandSurface, XwaylandSurfaceEventManager};
use std::pin::Pin;
use wlroots_sys::*;

#[derive(Debug, PartialEq, Eq)]
pub enum Surface {
  Layer(LayerSurface),
  Xdg(XdgSurface),
  Xwayland(XwaylandSurface),
  #[cfg(test)]
  Null,
}

impl Surface {
  pub(crate) fn from_wlr_surface(wlr_surface: *mut wlr_surface) -> Surface {
    if let Ok(xdg_surface) = XdgSurface::from_wlr_surface(wlr_surface) {
      Surface::Xdg(xdg_surface)
    } else if let Ok(layer_surface) = LayerSurface::from_wlr_surface(wlr_surface) {
      Surface::Layer(layer_surface)
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
  /// Returns the associated configure serial
  fn resize(&self, size: Size) -> u32;

  fn can_receive_focus(&self) -> bool;
  fn activated(&self) -> bool;
  /// Returns the associated configure serial
  fn set_activated(&self, activated: bool) -> u32;

  fn maximized(&self) -> bool;
  /// Returns the associated configure serial
  fn set_maximized(&self, maximized: bool) -> u32;
  fn fullscreen(&self) -> bool;
  /// Returns the associated configure serial
  fn set_fullscreen(&self, fullscreen: bool) -> u32;
  fn resizing(&self) -> bool;
  /// Returns the associated configure serial
  fn set_resizing(&self, resizing: bool) -> u32;

  fn app_id(&self) -> Option<String>;
  fn title(&self) -> Option<String>;

  fn ask_client_to_close(&self);
}

impl SurfaceExt for Surface {
  fn wlr_surface(&self) -> *mut wlr_surface {
    match self {
      Layer(surface) => surface.wlr_surface(),
      Xdg(surface) => surface.wlr_surface(),
      Xwayland(surface) => surface.wlr_surface(),
      #[cfg(test)]
      Null => std::ptr::null_mut(),
    }
  }

  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    match self {
      Layer(surface) => surface.parent_wlr_surface(),
      Xdg(surface) => surface.parent_wlr_surface(),
      Xwayland(surface) => surface.parent_wlr_surface(),
      #[cfg(test)]
      Null => None,
    }
  }

  fn buffer_displacement(&self) -> Displacement {
    match self {
      Layer(surface) => surface.buffer_displacement(),
      Xdg(surface) => surface.buffer_displacement(),
      Xwayland(surface) => surface.buffer_displacement(),
      #[cfg(test)]
      Null => Displacement::ZERO,
    }
  }

  fn parent_displacement(&self) -> Displacement {
    match self {
      Layer(surface) => surface.parent_displacement(),
      Xdg(surface) => surface.parent_displacement(),
      Xwayland(surface) => surface.parent_displacement(),
      #[cfg(test)]
      Null => Displacement::ZERO,
    }
  }

  fn extents(&self) -> Rectangle {
    match self {
      Layer(surface) => surface.extents(),
      Xdg(surface) => surface.extents(),
      Xwayland(surface) => surface.extents(),
      #[cfg(test)]
      Null => Rectangle::ZERO,
    }
  }

  fn move_to(&self, top_left: Point) {
    match self {
      Layer(surface) => surface.move_to(top_left),
      Xdg(surface) => surface.move_to(top_left),
      Xwayland(surface) => surface.move_to(top_left),
      #[cfg(test)]
      Null => {}
    }
  }

  fn resize(&self, size: Size) -> u32 {
    match self {
      Layer(surface) => surface.resize(size),
      Xdg(surface) => surface.resize(size),
      Xwayland(surface) => surface.resize(size),
      #[cfg(test)]
      Null => 1,
    }
  }

  fn can_receive_focus(&self) -> bool {
    match self {
      Layer(surface) => surface.can_receive_focus(),
      Xdg(surface) => surface.can_receive_focus(),
      Xwayland(surface) => surface.can_receive_focus(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn activated(&self) -> bool {
    match self {
      Layer(surface) => surface.activated(),
      Xdg(surface) => surface.activated(),
      Xwayland(surface) => surface.activated(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_activated(&self, activated: bool) -> u32 {
    match self {
      Layer(surface) => surface.set_activated(activated),
      Xdg(surface) => surface.set_activated(activated),
      Xwayland(surface) => surface.set_activated(activated),
      #[cfg(test)]
      Null => 1,
    }
  }

  fn maximized(&self) -> bool {
    match self {
      Layer(surface) => surface.maximized(),
      Xdg(surface) => surface.maximized(),
      Xwayland(surface) => surface.maximized(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_maximized(&self, maximized: bool) -> u32 {
    match self {
      Layer(surface) => surface.set_maximized(maximized),
      Xdg(surface) => surface.set_maximized(maximized),
      Xwayland(surface) => surface.set_maximized(maximized),
      #[cfg(test)]
      Null => 1,
    }
  }
  fn fullscreen(&self) -> bool {
    match self {
      Layer(surface) => surface.fullscreen(),
      Xdg(surface) => surface.fullscreen(),
      Xwayland(surface) => surface.fullscreen(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_fullscreen(&self, fullscreen: bool) -> u32 {
    match self {
      Layer(surface) => surface.set_fullscreen(fullscreen),
      Xdg(surface) => surface.set_fullscreen(fullscreen),
      Xwayland(surface) => surface.set_fullscreen(fullscreen),
      #[cfg(test)]
      Null => 1,
    }
  }
  fn resizing(&self) -> bool {
    match self {
      Layer(surface) => surface.resizing(),
      Xdg(surface) => surface.resizing(),
      Xwayland(surface) => surface.resizing(),
      #[cfg(test)]
      Null => false,
    }
  }
  fn set_resizing(&self, resizing: bool) -> u32 {
    match self {
      Layer(surface) => surface.set_resizing(resizing),
      Xdg(surface) => surface.set_resizing(resizing),
      Xwayland(surface) => surface.set_resizing(resizing),
      #[cfg(test)]
      Null => 1,
    }
  }

  fn app_id(&self) -> Option<String> {
    match self {
      Layer(surface) => surface.app_id(),
      Xdg(surface) => surface.app_id(),
      Xwayland(surface) => surface.app_id(),
      #[cfg(test)]
      Null => None,
    }
  }
  fn title(&self) -> Option<String> {
    match self {
      Layer(surface) => surface.title(),
      Xdg(surface) => surface.title(),
      Xwayland(surface) => surface.title(),
      #[cfg(test)]
      Null => None,
    }
  }

  fn ask_client_to_close(&self) {
    match self {
      Layer(surface) => surface.ask_client_to_close(),
      Xdg(surface) => surface.ask_client_to_close(),
      Xwayland(surface) => surface.ask_client_to_close(),
      #[cfg(test)]
      Null => {}
    }
  }
}

pub enum SurfaceEventManager {
  Layer(Pin<Box<LayerSurfaceEventManager>>),
  Xdg(Pin<Box<XdgSurfaceEventManager>>),
  Xwayland(Pin<Box<XwaylandSurfaceEventManager>>),
}

impl std::fmt::Debug for SurfaceEventManager {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
    write!(fmt, "SurfaceEventManager")
  }
}
