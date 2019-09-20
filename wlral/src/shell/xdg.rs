use crate::surface::*;
use crate::window_management_policy::{WindowManagementPolicy, WmManager};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::wl_display;
use wlroots_sys::*;

pub struct XdgEventHandler {
  wm_manager: Rc<RefCell<WmManager>>,
  surface_manager: Rc<RefCell<SurfaceManager>>,
}
impl XdgEventHandler {
  fn new_surface(&mut self, xdg_surface: *mut wlr_xdg_surface) {
    println!("new_surface");
    let surface = self
      .surface_manager
      .borrow_mut()
      .new_surface(SurfaceType::Xdg(xdg_surface));
    surface.bind_events(
      self.wm_manager.clone(),
      self.surface_manager.clone(),
      |event_manager| unsafe {
        event_manager.map(&mut (*xdg_surface).events.map);
        event_manager.unmap(&mut (*xdg_surface).events.unmap);
        event_manager.destroy(&mut (*xdg_surface).events.destroy);
      },
    );
    self
      .wm_manager
      .borrow_mut()
      .advise_new_window(surface.clone());
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
    wm_manager: Rc<RefCell<WmManager>>,
    surface_manager: Rc<RefCell<SurfaceManager>>,
    display: *mut wl_display,
  ) -> XdgManager {
    println!("XdgManager::init prebind");

    let xdg_shell = unsafe { wlr_xdg_shell_create(display) };

    let event_handler = Rc::new(RefCell::new(XdgEventHandler {
      wm_manager,
      surface_manager,
    }));

    let mut event_manager = XdgEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_surface(&mut (*xdg_shell).events.new_surface);
    }

    println!("XdgManager::init postbind");

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
