use crate::input::cursor::*;
use crate::input::event_filter::*;
use crate::input::keyboard::*;
use crate::input::seat::*;
use crate::output::*;
use crate::shell::xdg::*;
use crate::shell::xwayland::*;
use crate::surface::*;
use crate::window_management_policy::{WindowManagementPolicy, WmManager};
use std::cell::RefCell;
use std::env;
use std::ffi::{CStr, CString};
use std::rc::Rc;
use wayland_sys::server::*;
use wlroots_sys::*;

#[allow(unused)]
pub struct Compositor {
  display: *mut wl_display,
  backend: *mut wlr_backend,
  renderer: *mut wlr_renderer,
  compositor: *mut wlr_compositor,

  output_layout: *mut wlr_output_layout,
  output_manager: Rc<RefCell<OutputManager>>,

  surface_manager: Rc<RefCell<SurfaceManager>>,
  xdg_manager: XdgManager,
  xwayland_manager: XwaylandManager,

  seat_manager: SeatManager,
  cursor_manager: Rc<RefCell<CursorManager>>,
  keyboard_manager: Rc<RefCell<KeyboardManager>>,

  wm_manager: Rc<RefCell<WmManager>>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
}

impl Compositor {
  pub fn init() -> Result<Compositor, u32> {
    let wm_manager = Rc::new(RefCell::new(WmManager::new()));

    unsafe {
      // The Wayland display is managed by libwayland. It handles accepting
      // clients from the Unix socket, manging Wayland globals, and so on.
      let display = ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_create,) as *mut wl_display;
      // The backend is a wlroots feature which abstracts the underlying input and
      // output hardware. The autocreate option will choose the most suitable
      // backend based on the current environment, such as opening an X11 window
      // if an X11 server is running. The NULL argument here optionally allows you
      // to pass in a custom renderer if wlr_renderer doesn't meet your needs. The
      // backend uses the renderer, for example, to fall back to software cursors
      // if the backend does not support hardware cursors (some older GPUs
      // don't).
      let backend = wlr_backend_autocreate(display, None);

      // If we don't provide a renderer, autocreate makes a GLES2 renderer for us.
      // The renderer is responsible for defining the various pixel formats it
      // supports for shared memory, this configures that for clients.
      let renderer = wlr_backend_get_renderer(backend);
      wlr_renderer_init_wl_display(renderer, display);

      // This creates some hands-off wlroots interfaces. The compositor is
      // necessary for clients to allocate surfaces and the data device manager
      // handles the clipboard. Each of these wlroots interfaces has room for you
      // to dig your fingers in and play with their behavior if you want.
      let compositor = wlr_compositor_create(display, renderer);
      wlr_data_device_manager_create(display);

      // Configures a seat, which is a single "seat" at which a user sits and
      // operates the computer. This conceptually includes up to one keyboard,
      // pointer, touch, and drawing tablet device. We also rig up a listener to
      // let us know when new input devices are available on the backend.
      let seat = wlr_seat_create(display, CString::new("seat0").unwrap().as_ptr());

      let surface_manager = Rc::new(RefCell::new(SurfaceManager::init(seat)));

      // Creates an output layout, which a wlroots utility for working with an
      // arrangement of screens in a physical layout.
      let output_layout = wlr_output_layout_create();

      let output_manager = OutputManager::init(
        wm_manager.clone(),
        surface_manager.clone(),
        backend,
        renderer,
        output_layout,
      );

      let xdg_manager = XdgManager::init(wm_manager.clone(), surface_manager.clone(), display);
      let xwayland_manager = XwaylandManager::init(
        wm_manager.clone(),
        surface_manager.clone(),
        display,
        compositor,
      );

      let event_filter_manager = Rc::new(RefCell::new(EventFilterManager::new()));
      let cursor_manager = CursorManager::init(
        surface_manager.clone(),
        event_filter_manager.clone(),
        output_layout,
        seat,
      );
      let keyboard_manager = Rc::new(RefCell::new(KeyboardManager::init(
        event_filter_manager.clone(),
        seat,
      )));
      let seat_manager = SeatManager::init(
        backend,
        seat,
        cursor_manager.clone(),
        keyboard_manager.clone(),
      );

      // event_filter_manager
      //   .borrow_mut()
      //   .add_event_filter(Box::new(VtSwitchEventFilter::new(backend)));

      // Add a Unix socket to the Wayland display.
      let socket = ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_add_socket_auto, display);
      if socket.is_null() {
        // NOTE Rationale for panicking:
        // * Won't be in C land just yet, so it's safe to panic
        // * Can always be returned in a Result instead, but for now if you auto create
        //   it's assumed you can't recover.
        panic!("Unable to open wayland socket");
      }
      let socket_name = CStr::from_ptr(socket).to_string_lossy().into_owned();
      env::set_var("_WAYLAND_DISPLAY", socket_name.clone());

      // Start the backend. This will enumerate outputs and inputs, become the DRM
      // master, etc
      if !wlr_backend_start(backend) {
        wlr_backend_destroy(backend);
        ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_destroy, display);
        return Err(2);
      }

      Ok(Compositor {
        display,
        backend,
        renderer,
        compositor,

        output_layout,
        output_manager,

        surface_manager,
        xdg_manager,
        xwayland_manager,

        seat_manager,
        cursor_manager,
        keyboard_manager,

        wm_manager,
        event_filter_manager,
      })
    }
  }

  pub fn output_manager(&self) -> Rc<RefCell<OutputManager>> {
    self.output_manager.clone()
  }

  pub fn add_event_filter(&mut self, filter: Box<dyn EventFilter>) {
    self
      .event_filter_manager
      .borrow_mut()
      .add_event_filter(filter)
  }

  pub fn run<T>(self, window_management_policy: T)
  where
    T: 'static + WindowManagementPolicy,
  {
    self
      .wm_manager
      .borrow_mut()
      .set_policy(window_management_policy);

    unsafe {
      // if (startup_cmd) {
      //   if (fork() == 0) {
      //     execl("/bin/sh", "/bin/sh", "-c", startup_cmd, (void *)NULL);
      //   }
      // }

      // Run the Wayland event loop. This does not return until you exit the
      // compositor. Starting the backend rigged up all of the necessary event
      // loop configuration to listen to libinput events, DRM events, generate
      // frame events at the refresh rate, and so on.
      // wlr_log(WLR_INFO, "Running Wayland compositor on WAYLAND_DISPLAY=%s",
      //		 socket);
      ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_run, self.display);

      // Once wl_display_run returns, we shut down the server.
      ffi_dispatch!(
        WAYLAND_SERVER_HANDLE,
        wl_display_destroy_clients,
        self.display
      );
      ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_destroy, self.display);
    }
  }
}
