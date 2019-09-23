use crate::geometry::*;
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceExt};
use crate::window::WindowEvents;
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::{WindowManager, WindowManagerExt};
use log::{debug, info};
use std::cell::RefCell;
use std::env;
use std::ffi::CStr;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::wl_display;
use wlroots_sys::*;

#[derive(PartialEq, Eq)]
pub struct XwaylandSurface(*mut wlr_xwayland_surface);

impl XwaylandSurface {
  pub(crate) fn from_wlr_surface(wlr_surface: *mut wlr_surface) -> Result<XwaylandSurface, ()> {
    unsafe {
      if wlr_surface_is_xwayland_surface(wlr_surface) {
        let xwayland_surface = wlr_xwayland_surface_from_wlr_surface(wlr_surface);
        Ok(XwaylandSurface(xwayland_surface))
      } else {
        Err(())
      }
    }
  }
}

impl SurfaceExt for XwaylandSurface {
  fn wlr_surface(&self) -> *mut wlr_surface {
    unsafe { (*self.0).surface }
  }

  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    None
  }

  fn buffer_displacement(&self) -> Displacement {
    Displacement::ZERO
  }

  fn parent_displacement(&self) -> Displacement {
    Displacement::ZERO
  }

  fn extents(&self) -> Rectangle {
    unsafe {
      Rectangle {
        top_left: Point {
          x: (*self.0).x as i32,
          y: (*self.0).y as i32,
        },
        size: Size {
          width: (*self.0).width as i32,
          height: (*self.0).height as i32,
        },
      }
    }
  }

  fn move_to(&self, top_left: Point) {
    unsafe {
      wlr_xwayland_surface_configure(
        self.0,
        top_left.x as i16,
        top_left.y as i16,
        (*self.0).width,
        (*self.0).height,
      );
    }
  }

  fn resize(&self, size: Size) {
    unsafe {
      wlr_xwayland_surface_configure(
        self.0,
        (*self.0).x,
        (*self.0).y,
        size.width as u16,
        size.height as u16,
      );
    }
  }

  fn can_receive_focus(&self) -> bool {
    // TODO: Is this true?
    true
  }
  fn activated(&self) -> bool {
    false
  }
  fn set_activated(&self, activated: bool) {
    unsafe {
      wlr_xwayland_surface_activate(self.0, activated);
    }
  }

  fn maximized(&self) -> bool {
    unsafe { (*self.0).maximized_vert && (*self.0).maximized_horz }
  }
  fn set_maximized(&self, maximized: bool) {
    unsafe {
      wlr_xwayland_surface_set_maximized(self.0, maximized);
    }
  }
  fn fullscreen(&self) -> bool {
    unsafe { (*self.0).fullscreen }
  }
  fn set_fullscreen(&self, fullscreen: bool) {
    unsafe {
      wlr_xwayland_surface_set_fullscreen(self.0, fullscreen);
    }
  }
  fn resizing(&self) -> bool {
    false
  }
  fn set_resizing(&self, _resizing: bool) {}

  fn ask_client_to_close(&self) {
    unsafe {
      wlr_xwayland_surface_close(self.0);
    }
  }
}

pub struct XwaylandEventHandler {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  output_manager: Rc<RefCell<dyn OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
}
impl XwaylandEventHandler {
  fn new_surface(&mut self, xwayland_surface: *mut wlr_xwayland_surface) {
    debug!("new_surface");
    let surface = self
      .window_manager
      .new_window(Surface::Xwayland(XwaylandSurface(xwayland_surface)));

    surface.bind_events(
      self.wm_policy_manager.clone(),
      self.output_manager.clone(),
      self.window_manager.clone(),
      self.cursor_manager.clone(),
      |event_manager| unsafe {
        event_manager.map(&mut (*xwayland_surface).events.map);
        event_manager.unmap(&mut (*xwayland_surface).events.unmap);
        event_manager.destroy(&mut (*xwayland_surface).events.destroy);
        event_manager.request_move(&mut (*xwayland_surface).events.request_move);
        event_manager.request_resize(&mut (*xwayland_surface).events.request_resize);
        event_manager.request_maximize(&mut (*xwayland_surface).events.request_maximize);
        event_manager.request_fullscreen(&mut (*xwayland_surface).events.request_fullscreen);
        // TODO: minimize?
      },
    );

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_new_window(surface.clone());
  }
}

wayland_listener!(
  pub XwaylandEventManager,
  Rc<RefCell<XwaylandEventHandler>>,
  [
     new_surface => new_surface_func: |this: &mut XwaylandEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().new_surface(data as _)
     };
  ]
);

#[allow(unused)]
pub struct XwaylandManager {
  xwayland: *mut wlr_xwayland,

  event_manager: Pin<Box<XwaylandEventManager>>,
  event_handler: Rc<RefCell<XwaylandEventHandler>>,
}

impl XwaylandManager {
  pub(crate) fn init(
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    output_manager: Rc<RefCell<dyn OutputManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    display: *mut wl_display,
    compositor: *mut wlr_compositor,
  ) -> XwaylandManager {
    debug!("XwaylandManager::init");

    let xwayland = unsafe { &mut *wlr_xwayland_create(display, compositor, true) };

    let socket_name = unsafe {
      CStr::from_ptr(xwayland.display_name.as_ptr())
        .to_string_lossy()
        .into_owned()
    };
    env::set_var("_DISPLAY", socket_name.clone());
    info!("DISPLAY={}", socket_name.clone());

    let event_handler = Rc::new(RefCell::new(XwaylandEventHandler {
      wm_policy_manager,
      output_manager,
      window_manager,
      cursor_manager,
    }));

    let mut event_manager = XwaylandEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_surface(&mut xwayland.events.new_surface);
    }

    XwaylandManager {
      xwayland,

      event_manager,
      event_handler,
    }
  }
}

impl Drop for XwaylandManager {
  fn drop(&mut self) {
    unsafe {
      wlr_xwayland_destroy(self.xwayland);
    }
  }
}
