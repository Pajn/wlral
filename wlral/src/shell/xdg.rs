use crate::geometry::*;
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window::*;
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::{WindowManager, WindowManagerExt};
use log::debug;
use std::cell::RefCell;
use std::ffi::CStr;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::wl_display;
use wlroots_sys::*;

enum XdgSurfaceType {
  None,
  Toplevel(*mut wlr_xdg_toplevel),
  Popup(*mut wlr_xdg_popup),
}
use XdgSurfaceType::{Popup, Toplevel};

#[derive(PartialEq, Eq)]
pub struct XdgSurface(*mut wlr_xdg_surface);

impl XdgSurface {
  pub(crate) fn from_wlr_surface(wlr_surface: *mut wlr_surface) -> Result<XdgSurface, ()> {
    unsafe {
      if wlr_surface_is_xdg_surface(wlr_surface) {
        let xdg_surface = wlr_xdg_surface_from_wlr_surface(wlr_surface);
        Ok(XdgSurface(xdg_surface))
      } else {
        Err(())
      }
    }
  }

  fn get_type(&self) -> XdgSurfaceType {
    unsafe {
      match (*self.0).role {
        role if role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_TOPLEVEL => {
          XdgSurfaceType::Toplevel((*self.0).__bindgen_anon_1.toplevel)
        }
        role if role == wlr_xdg_surface_role_WLR_XDG_SURFACE_ROLE_POPUP => {
          XdgSurfaceType::Popup((*self.0).__bindgen_anon_1.popup)
        }
        _ => XdgSurfaceType::None,
      }
    }
  }
}

impl SurfaceExt for XdgSurface {
  fn wlr_surface(&self) -> *mut wlr_surface {
    unsafe { (*self.0).surface }
  }

  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    match self.get_type() {
      Popup(popup) => unsafe { Some((*popup).parent) },
      _ => None,
    }
  }

  fn buffer_displacement(&self) -> Displacement {
    let surface = unsafe { &*self.wlr_surface() };

    let buffer_position = Point {
      x: surface.current.dx,
      y: surface.current.dy,
    };

    self.extents().top_left() - buffer_position
  }

  fn parent_displacement(&self) -> Displacement {
    match self.get_type() {
      Popup(popup) => unsafe {
        let parent = wlr_xdg_surface_from_wlr_surface((*popup).parent);
        let mut parent_geo = Rectangle::ZERO.into();

        wlr_xdg_surface_get_geometry(parent, &mut parent_geo);

        Displacement {
          dx: parent_geo.x + (*popup).geometry.x,
          dy: parent_geo.y + (*popup).geometry.y,
        }
      },
      _ => Displacement::ZERO,
    }
  }

  fn extents(&self) -> Rectangle {
    unsafe {
      let mut wlr_box = Rectangle::ZERO.into();
      wlr_xdg_surface_get_geometry(self.0, &mut wlr_box);
      Rectangle::from(wlr_box)
    }
  }

  fn move_to(&self, _top_left: Point) {}

  fn resize(&self, size: Size) -> u32 {
    match self.get_type() {
      Toplevel(_) => unsafe {
        wlr_xdg_toplevel_set_size(self.0, size.width as u32, size.height as u32)
      },
      _ => 0,
    }
  }

  fn can_receive_focus(&self) -> bool {
    match self.get_type() {
      Toplevel(_) => true,
      _ => false,
    }
  }
  fn activated(&self) -> bool {
    match self.get_type() {
      Toplevel(toplevel) => unsafe { (*toplevel).current.activated },
      _ => false,
    }
  }
  fn set_activated(&self, activated: bool) -> u32 {
    match self.get_type() {
      Toplevel(_) => unsafe { wlr_xdg_toplevel_set_activated(self.0, activated) },
      _ => 0,
    }
  }

  fn maximized(&self) -> bool {
    match self.get_type() {
      Toplevel(toplevel) => unsafe { (*toplevel).current.maximized },
      _ => false,
    }
  }
  fn set_maximized(&self, maximized: bool) -> u32 {
    match self.get_type() {
      Toplevel(_) => unsafe { wlr_xdg_toplevel_set_maximized(self.0, maximized) },
      _ => 0,
    }
  }
  fn fullscreen(&self) -> bool {
    match self.get_type() {
      Toplevel(toplevel) => unsafe { (*toplevel).current.fullscreen },
      _ => false,
    }
  }
  fn set_fullscreen(&self, fullscreen: bool) -> u32 {
    match self.get_type() {
      Toplevel(_) => unsafe { wlr_xdg_toplevel_set_fullscreen(self.0, fullscreen) },
      _ => 0,
    }
  }
  fn resizing(&self) -> bool {
    match self.get_type() {
      Toplevel(toplevel) => unsafe { (*toplevel).current.resizing },
      _ => false,
    }
  }
  fn set_resizing(&self, resizing: bool) -> u32 {
    match self.get_type() {
      Toplevel(_) => unsafe { wlr_xdg_toplevel_set_resizing(self.0, resizing) },
      _ => 0,
    }
  }

  fn app_id(&self) -> Option<String> {
    match self.get_type() {
      Toplevel(toplevel) => unsafe {
        Some(
          CStr::from_ptr((*toplevel).app_id)
            .to_string_lossy()
            .into_owned(),
        )
      },
      _ => None,
    }
  }
  fn title(&self) -> Option<String> {
    match self.get_type() {
      Toplevel(toplevel) => unsafe {
        Some(
          CStr::from_ptr((*toplevel).title)
            .to_string_lossy()
            .into_owned(),
        )
      },
      _ => None,
    }
  }

  fn ask_client_to_close(&self) {
    match self.get_type() {
      Toplevel(_) => unsafe {
        wlr_xdg_toplevel_send_close(self.0);
      },
      Popup(_) => unsafe {
        wlr_xdg_popup_destroy(self.0);
      },
      _ => {}
    }
  }
}

wayland_listener!(
  pub XdgSurfaceEventManager,
  WindowEventHandler,
  [
    map => map_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.map()
    };
    unmap => unmap_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.unmap()
    };
    destroy => destroy_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.destroy();
    };
    commit => commit_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      if let Some(window) = handler.window.upgrade() {
        if let Surface::Xdg(ref xdg_surface) = window.surface {
          handler.commit(WindowCommitEvent {
            serial: (*xdg_surface.0).configure_serial,
          });
        }
      }
    };
    request_move => request_move_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_move();
    };
    request_resize => request_resize_func: |this: &mut XdgSurfaceEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      let event: *mut wlr_xdg_toplevel_resize_event = data as _;
      handler.request_resize(WindowResizeEvent {
        edges: (*event).edges,
      });
    };
    request_maximize => request_maximize_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      if let Some(window) = handler.window.upgrade() {
        if let Surface::Xdg(ref xdg_surface) = window.surface {
          if let Toplevel(toplevel) = xdg_surface.get_type() {
            handler.request_maximize(WindowMaximizeEvent {
              maximize: (*toplevel).client_pending.maximized
            });
          }
        }
      }
    };
    request_fullscreen => request_fullscreen_func: |this: &mut XdgSurfaceEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      let event: *mut wlr_xdg_toplevel_set_fullscreen_event = data as _;
      handler.request_fullscreen(WindowFullscreenEvent {
        fullscreen: (*event).fullscreen,
        output: (*event).output.as_mut().map(|o| o as *mut _),
      });
    };
    request_minimize => request_minimize_func: |this: &mut XdgSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_minimize();
    };
  ]
);

pub struct XdgEventHandler {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  output_manager: Rc<RefCell<dyn OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
}
impl XdgEventHandler {
  fn new_surface(&mut self, xdg_surface: *mut wlr_xdg_surface) {
    debug!("new_surface");

    let window = self
      .window_manager
      .new_window(Surface::Xdg(XdgSurface(xdg_surface)));

    let mut event_manager = XdgSurfaceEventManager::new(WindowEventHandler {
      wm_policy_manager: self.wm_policy_manager.clone(),
      output_manager: self.output_manager.clone(),
      window_manager: self.window_manager.clone(),
      cursor_manager: self.cursor_manager.clone(),
      window: Rc::downgrade(&window),
    });

    unsafe {
      event_manager.map(&mut (*xdg_surface).events.map);
      event_manager.unmap(&mut (*xdg_surface).events.unmap);
      event_manager.destroy(&mut (*xdg_surface).events.destroy);
      event_manager.commit(&mut (*(*xdg_surface).surface).events.commit);

      match XdgSurface(xdg_surface).get_type() {
        Toplevel(toplevel) => {
          let toplevel = &mut *toplevel;

          event_manager.request_move(&mut toplevel.events.request_move);
          event_manager.request_resize(&mut toplevel.events.request_resize);
          event_manager.request_maximize(&mut toplevel.events.request_maximize);
          event_manager.request_fullscreen(&mut toplevel.events.request_fullscreen);
          event_manager.request_minimize(&mut toplevel.events.request_minimize);
        }
        _ => {}
      }
    }

    *window.event_manager.borrow_mut() = Some(SurfaceEventManager::Xdg(event_manager));

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_new_window(window);
  }
}

wayland_listener!(
  pub XdgEventManager,
  Rc<RefCell<XdgEventHandler>>,
  [
     new_surface => new_surface_func: |this: &mut XdgEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().new_surface(data as _)
     };
  ]
);

#[allow(unused)]
pub struct XdgManager {
  xdg_shell: *mut wlr_xdg_shell,

  event_manager: Pin<Box<XdgEventManager>>,
  event_handler: Rc<RefCell<XdgEventHandler>>,
}

impl XdgManager {
  pub(crate) fn init(
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    output_manager: Rc<RefCell<dyn OutputManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
    display: *mut wl_display,
  ) -> XdgManager {
    debug!("XdgManager::init");

    let xdg_shell = unsafe { wlr_xdg_shell_create(display) };

    let event_handler = Rc::new(RefCell::new(XdgEventHandler {
      wm_policy_manager,
      output_manager,
      window_manager,
      cursor_manager,
    }));

    let mut event_manager = XdgEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_surface(&mut (*xdg_shell).events.new_surface);
    }

    XdgManager {
      xdg_shell,

      event_manager,
      event_handler,
    }
  }
}

impl Drop for XdgManager {
  fn drop(&mut self) {
    unsafe {
      wlr_xdg_shell_destroy(self.xdg_shell);
    }
  }
}
