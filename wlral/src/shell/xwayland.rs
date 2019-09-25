use crate::geometry::*;
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window::*;
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

/// As XWayland does not support serials we use this constant value
/// as a ponyfill
const CONFIGURE_SERIAL: u32 = 1;

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

  fn resize(&self, size: Size) -> u32 {
    unsafe {
      wlr_xwayland_surface_configure(
        self.0,
        (*self.0).x,
        (*self.0).y,
        size.width as u16,
        size.height as u16,
      );
      CONFIGURE_SERIAL
    }
  }

  fn can_receive_focus(&self) -> bool {
    // TODO: Is this true?
    true
  }
  fn activated(&self) -> bool {
    false
  }
  fn set_activated(&self, activated: bool) -> u32 {
    unsafe {
      wlr_xwayland_surface_activate(self.0, activated);
      CONFIGURE_SERIAL
    }
  }

  fn maximized(&self) -> bool {
    unsafe { (*self.0).maximized_vert && (*self.0).maximized_horz }
  }
  fn set_maximized(&self, maximized: bool) -> u32 {
    unsafe {
      wlr_xwayland_surface_set_maximized(self.0, maximized);
      CONFIGURE_SERIAL
    }
  }
  fn fullscreen(&self) -> bool {
    unsafe { (*self.0).fullscreen }
  }
  fn set_fullscreen(&self, fullscreen: bool) -> u32 {
    unsafe {
      wlr_xwayland_surface_set_fullscreen(self.0, fullscreen);
      CONFIGURE_SERIAL
    }
  }
  fn resizing(&self) -> bool {
    false
  }
  fn set_resizing(&self, _resizing: bool) -> u32 {
    CONFIGURE_SERIAL
  }

  fn ask_client_to_close(&self) {
    unsafe {
      wlr_xwayland_surface_close(self.0);
    }
  }
}

wayland_listener!(
  pub XwaylandSurfaceEventManager,
  WindowEventHandler,
  [
    map => map_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.map()
    };
    unmap => unmap_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.unmap()
    };
    destroy => destroy_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.destroy();
    };
    commit => commit_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.commit(WindowCommitEvent {
        serial: CONFIGURE_SERIAL,
      });
    };
    request_move => request_move_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_move();
    };
    request_resize => request_resize_func: |this: &mut XwaylandSurfaceEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      let event: *mut wlr_xwayland_resize_event = data as _;
      handler.request_resize(WindowResizeEvent {
        edges: (*event).edges,
      });
    };
    request_maximize => request_maximize_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_maximize(WindowMaximizeEvent {
        maximize: true,
      });
    };
    request_fullscreen => request_fullscreen_func: |this: &mut XwaylandSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_fullscreen(WindowFullscreenEvent {
        fullscreen: true,
        output: None,
      });
    };
  ]
);

pub struct XwaylandEventHandler {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  output_manager: Rc<RefCell<dyn OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
}
impl XwaylandEventHandler {
  fn new_surface(&mut self, xwayland_surface: *mut wlr_xwayland_surface) {
    debug!("new_surface");
    let window = self
      .window_manager
      .new_window(Surface::Xwayland(XwaylandSurface(xwayland_surface)));

    let mut event_manager = XwaylandSurfaceEventManager::new(WindowEventHandler {
      wm_policy_manager: self.wm_policy_manager.clone(),
      output_manager: self.output_manager.clone(),
      window_manager: self.window_manager.clone(),
      cursor_manager: self.cursor_manager.clone(),
      window: Rc::downgrade(&window),
    });

    unsafe {
      let xwayland_surface = &mut *xwayland_surface;
      event_manager.map(&mut xwayland_surface.events.map);
      event_manager.unmap(&mut xwayland_surface.events.unmap);
      event_manager.destroy(&mut xwayland_surface.events.destroy);
      event_manager.commit(&mut (*xwayland_surface.surface).events.commit);
      event_manager.request_move(&mut xwayland_surface.events.request_move);
      event_manager.request_resize(&mut xwayland_surface.events.request_resize);
      event_manager.request_maximize(&mut xwayland_surface.events.request_maximize);
      event_manager.request_fullscreen(&mut xwayland_surface.events.request_fullscreen);
      // TODO: minimize?
    }

    *window.event_manager.borrow_mut() = Some(SurfaceEventManager::Xwayland(event_manager));

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_new_window(window);
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
