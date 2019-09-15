use crate::input::cursor::*;
use crate::input::keyboard::*;
use crate::input::seat::*;
use crate::output::*;
use crate::shell::xdg::*;
use crate::surface::*;
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

  output_layout: *mut wlr_output_layout,
  output_manager: OutputManager,

  surface_manager: Rc<RefCell<SurfaceManager>>,
  xdg_manager: XdgManager,

  seat_manager: SeatManager,
  cursor_manager: Rc<RefCell<CursorManager>>,
  keyboard_manager: Rc<RefCell<KeyboardManager>>,
}

impl Compositor {
  pub fn init() -> Result<Compositor, u32> {
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
      wlr_compositor_create(display, renderer);
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

      let output_manager =
        OutputManager::init(backend, renderer, surface_manager.clone(), output_layout);

      // Set up our list of views and the xdg-shell. The xdg-shell is a Wayland
      // protocol which is used for application windows.
      // wl_list_init(&server.views);
      // let xdg_shell = wlr_xdg_shell_create(display);
      // server.new_xdg_surface.notify = server_new_xdg_surface;
      // wl_signal_add(&server.xdg_shell->events.new_surface,
      // 		&server.new_xdg_surface);
      let xdg_manager = XdgManager::init(display, surface_manager.clone());

      let cursor_manager = Rc::new(RefCell::new(CursorManager::init(
        surface_manager.clone(),
        output_layout,
        seat,
      )));
      let keyboard_manager = Rc::new(RefCell::new(KeyboardManager::init(seat)));
      let seat_manager = SeatManager::init(
        backend,
        seat,
        cursor_manager.clone(),
        keyboard_manager.clone(),
      );

      // * Configures a seat, which is a single "seat" at which a user sits and
      // * operates the computer. This conceptually includes up to one keyboard,
      // * pointer, touch, and drawing tablet device. We also rig up a listener to
      // * let us know when new input devices are available on the backend.
      // wl_list_init(&server.keyboards);
      // server.new_input.notify = server_new_input;
      // wl_signal_add(&server.backend->events.new_input, &server.new_input);

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

        output_layout,
        output_manager,

        surface_manager,
        xdg_manager,

        seat_manager,
        cursor_manager,
        keyboard_manager,
      })
    }
  }

  pub fn run(self) {
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
