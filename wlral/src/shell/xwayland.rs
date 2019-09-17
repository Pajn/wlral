use crate::surface::*;
use std::cell::RefCell;
use std::env;
use std::ffi::CStr;
use std::pin::Pin;
use std::rc::Rc;
use wayland_sys::server::wl_display;
use wlroots_sys::*;

pub struct XwaylandEventHandler {
  surface_manager: Rc<RefCell<SurfaceManager>>,
}
impl XwaylandEventHandler {
  fn new_surface(&mut self, xwayland_surface: *mut wlr_xwayland_surface) {
    println!("new_surface");
    let surface = self
      .surface_manager
      .borrow_mut()
      .new_surface(SurfaceType::Xwayland(xwayland_surface));
    surface.bind_events(self.surface_manager.clone(), |event_manager| unsafe {
      event_manager.map(&mut (*xwayland_surface).events.map);
      event_manager.unmap(&mut (*xwayland_surface).events.unmap);
      event_manager.destroy(&mut (*xwayland_surface).events.destroy);
    })
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
  event_manager: Pin<Box<XwaylandEventManager>>,
  event_handler: Rc<RefCell<XwaylandEventHandler>>,
}

impl XwaylandManager {
  pub fn init(
    display: *mut wl_display,
    compositor: *mut wlr_compositor,
    surface_manager: Rc<RefCell<SurfaceManager>>,
  ) -> XwaylandManager {
    println!("XwaylandManager::init prebind");

    let xwayland = unsafe { &mut *wlr_xwayland_create(display, compositor, true) };

    let socket_name = unsafe {
      CStr::from_ptr(xwayland.display_name.as_ptr())
        .to_string_lossy()
        .into_owned()
    };
    env::set_var("_DISPLAY", socket_name.clone());
    println!("{}", socket_name.clone());

    let event_handler = Rc::new(RefCell::new(XwaylandEventHandler {
      surface_manager: surface_manager.clone(),
    }));

    let mut event_manager = XwaylandEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_surface(&mut xwayland.events.new_surface);
    }

    println!("XwaylandManager::init postbind");

    XwaylandManager {
      event_manager,
      event_handler,
    }
  }
}
