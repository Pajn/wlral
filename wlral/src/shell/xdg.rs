use crate::surface::*;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::{signal::wl_signal_add, wl_display};
use wlroots_sys::*;

pub struct XdgEventHandler {
  surface_manager: Rc<RefCell<SurfaceManager>>,
}
impl XdgEventHandler {
  fn new_surface(&mut self, xdg_surface: *mut wlr_xdg_surface) {
    println!("new_surface");
    let surface = self
      .surface_manager
      .borrow_mut()
      .new_surface(SurfaceType::Xdg(xdg_surface));
    surface.bind_events(self.surface_manager.clone(), |event_manager| unsafe {
      wl_signal_add(&mut (*xdg_surface).events.map, event_manager.map());
      wl_signal_add(&mut (*xdg_surface).events.unmap, event_manager.unmap());
      wl_signal_add(&mut (*xdg_surface).events.destroy, event_manager.destroy());
    })
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
  event_manager: Pin<Box<XdgEventManager>>,
  event_handler: Rc<RefCell<XdgEventHandler>>,
}

impl XdgManager {
  pub fn init(
    display: *mut wl_display,
    surface_manager: Rc<RefCell<SurfaceManager>>,
  ) -> XdgManager {
    println!("XdgManager::init prebind");

    let xdg_shell = unsafe { wlr_xdg_shell_create(display) };

    let event_handler = Rc::new(RefCell::new(XdgEventHandler {
      surface_manager: surface_manager.clone(),
    }));

    let mut event_manager = XdgEventManager::new(event_handler.clone());
    unsafe {
      wl_signal_add(
        &mut (*xdg_shell).events.new_surface,
        event_manager.new_surface(),
      );
    }

    println!("XdgManager::init postbind");

    XdgManager {
      event_manager,
      event_handler,
    }
  }
}
