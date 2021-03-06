use crate::{
  config::ConfigManager,
  input::cursor::*,
  input::event_filter::*,
  input::keyboard::*,
  input::seat::*,
  output_management_protocol::OutputManagementProtocol,
  output_manager::OutputManager,
  shell::layer::*,
  shell::xdg::*,
  shell::xwayland::*,
  window_management_policy::{WindowManagementPolicy, WmPolicyManager},
  window_manager::{WindowManager, WindowManagerExt},
};
use log::{debug, error};
use std::{
  cell::RefCell,
  env,
  ffi::{CStr, CString},
  rc::Rc,
};
use wayland_sys::server::*;
use wlroots_sys::*;

#[allow(unused)]
pub struct Compositor {
  config_manager: Rc<ConfigManager>,

  display: *mut wl_display,
  backend: *mut wlr_backend,
  renderer: *mut wlr_renderer,
  compositor: *mut wlr_compositor,

  output_layout: *mut wlr_output_layout,
  output_manager: Rc<OutputManager>,
  output_management_protocol: RefCell<Option<Rc<OutputManagementProtocol>>>,

  window_manager: Rc<WindowManager>,
  layer_shell_manager: LayerShellManager,
  xdg_manager: XdgManager,
  xwayland_manager: XwaylandManager,

  seat_manager: Rc<SeatManager>,
  cursor_manager: Rc<CursorManager>,
  keyboard_manager: Rc<KeyboardManager>,

  wm_policy_manager: Rc<WmPolicyManager>,
  event_filter_manager: Rc<EventFilterManager>,
}

impl Compositor {
  pub fn init() -> Compositor {
    let wm_policy_manager = Rc::new(WmPolicyManager::new());
    let config_manager = Rc::new(ConfigManager::default());

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
      wlr_gamma_control_manager_v1_create(display);
      wlr_gtk_primary_selection_device_manager_create(display);

      // Configures a seat, which is a single "seat" at which a user sits and
      // operates the computer. This conceptually includes up to one keyboard,
      // pointer, touch, and drawing tablet device. We also rig up a listener to
      // let us know when new input devices are available on the backend.
      let seat_name = CString::new("seat0").unwrap();
      let seat = wlr_seat_create(display, seat_name.as_ptr());

      let seat_manager = SeatManager::init(display, backend, seat);
      let window_manager = Rc::new(WindowManager::init(
        wm_policy_manager.clone(),
        seat_manager.clone(),
        display,
      ));

      // Creates an output layout, which a wlroots utility for working with an
      // arrangement of screens in a physical layout.
      let output_layout = wlr_output_layout_create();

      let output_manager = OutputManager::init(
        config_manager.clone(),
        wm_policy_manager.clone(),
        window_manager.clone(),
        display,
        backend,
        renderer,
        output_layout,
      );
      window_manager.set_output_manager(output_manager.clone());

      let event_filter_manager = Rc::new(EventFilterManager::new());
      let cursor_manager = CursorManager::init(
        output_manager.clone(),
        window_manager.clone(),
        seat_manager.clone(),
        event_filter_manager.clone(),
        output_layout,
      );
      let keyboard_manager = KeyboardManager::init(
        config_manager.clone(),
        seat_manager.clone(),
        event_filter_manager.clone(),
      );

      let layer_shell_manager = LayerShellManager::init(
        wm_policy_manager.clone(),
        output_manager.clone(),
        window_manager.clone(),
        cursor_manager.clone(),
        display,
      );
      let xdg_manager = XdgManager::init(
        wm_policy_manager.clone(),
        output_manager.clone(),
        window_manager.clone(),
        cursor_manager.clone(),
        display,
      );
      let xwayland_manager = XwaylandManager::init(
        wm_policy_manager.clone(),
        output_manager.clone(),
        window_manager.clone(),
        cursor_manager.clone(),
        display,
        compositor,
      );

      event_filter_manager.add_event_filter(Box::new(VtSwitchEventFilter::new(backend)));

      wlr_export_dmabuf_manager_v1_create(display);
      wlr_screencopy_manager_v1_create(display);
      wlr_data_control_manager_v1_create(display);
      wlr_primary_selection_v1_device_manager_create(display);

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
      env::set_var("WAYLAND_DISPLAY", socket_name.clone());
      env::set_var("_WAYLAND_DISPLAY", socket_name);

      debug!("Compositor::init");

      Compositor {
        config_manager,

        display,
        backend,
        renderer,
        compositor,

        output_layout,
        output_manager,
        output_management_protocol: RefCell::new(None),

        window_manager,
        layer_shell_manager,
        xdg_manager,
        xwayland_manager,

        seat_manager,
        cursor_manager,
        keyboard_manager,

        wm_policy_manager,
        event_filter_manager,
      }
    }
  }

  pub fn config_manager(&self) -> Rc<ConfigManager> {
    self.config_manager.clone()
  }

  pub fn output_manager(&self) -> Rc<OutputManager> {
    self.output_manager.clone()
  }

  pub fn window_manager(&self) -> Rc<WindowManager> {
    self.window_manager.clone()
  }

  pub fn cursor_manager(&self) -> Rc<CursorManager> {
    self.cursor_manager.clone()
  }

  pub fn output_management_protocol(&self) -> Option<Rc<OutputManagementProtocol>> {
    self.output_management_protocol.borrow().clone()
  }

  pub fn enable_output_management_protocol(
    &self,
    pending_test_timeout_ms: u32,
  ) -> Result<Rc<OutputManagementProtocol>, ()> {
    if self.output_management_protocol.borrow().is_some() {
      error!("Compositor::enable_output_management_protocol: output management protocol is already enabled");
      return Err(());
    }
    let protocol =
      OutputManagementProtocol::init(self.output_manager.clone(), pending_test_timeout_ms);
    self
      .output_management_protocol
      .borrow_mut()
      .replace(protocol.clone());

    Ok(protocol)
  }

  pub fn add_event_filter(&mut self, filter: Box<dyn EventFilter>) {
    self.event_filter_manager.add_event_filter(filter)
  }

  pub fn run<T>(self, window_management_policy: T) -> Result<(), u32>
  where
    T: 'static + WindowManagementPolicy + EventFilter,
  {
    let window_management_policy = Rc::new(window_management_policy);
    self
      .wm_policy_manager
      .set_policy(window_management_policy.clone());
    self
      .event_filter_manager
      .add_event_filter(Box::new(window_management_policy));

    debug!("Compositor::run");

    unsafe {
      // Start the backend. This will enumerate outputs and inputs, become the DRM
      // master, etc
      if !wlr_backend_start(self.backend) {
        wlr_backend_destroy(self.backend);
        ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_display_destroy, self.display);
        return Err(2);
      }

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
    Ok(())
  }
}
