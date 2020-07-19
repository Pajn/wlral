use crate::geometry::{Displacement, FPoint, Point, Rectangle, Size};
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window_management_policy::*;
use crate::{
  event::{Event, EventOnce},
  output::Output,
  window_manager::{WindowLayer, WindowManager},
};
use bitflags::bitflags;
use log::{debug, error};
use std::cell::{Ref, RefCell};
use std::cmp::PartialEq;
use std::collections::BTreeMap;
use std::{
  borrow::Cow,
  ffi::{CStr, CString, NulError},
  pin::Pin,
  rc::{Rc, Weak},
};
use wlroots_sys::*;

bitflags! {
  pub struct WindowEdge: u32 {
    const NONE   = 0b0000;
    const TOP    = 0b0001;
    const BOTTOM = 0b0010;
    const LEFT   = 0b0100;
    const RIGHT  = 0b1000;
  }
}

#[derive(Debug)]
pub struct PendingUpdate {
  top_left: Point,
}

#[derive(Debug)]
pub struct MinimizeTarget {
  surface: *mut wlr_surface,
  rectangle: Rectangle,
}

#[derive(Debug)]
pub struct Window {
  pub(crate) output_manager: Rc<OutputManager>,
  pub(crate) window_manager: Rc<WindowManager>,

  pub(crate) surface: Surface,
  pub(crate) layer: WindowLayer,
  pub(crate) mapped: RefCell<bool>,
  pub(crate) top_left: RefCell<Point>,

  pub(crate) outputs: RefCell<Vec<Rc<Output>>>,
  pub(crate) minimize_targets: RefCell<Vec<MinimizeTarget>>,

  pub(crate) pending_updates: RefCell<BTreeMap<u32, PendingUpdate>>,

  pub(crate) on_entered_output: Event<Rc<Output>>,
  pub(crate) on_left_output: Event<Rc<Output>>,
  pub(crate) on_destroy: EventOnce<()>,

  pub(crate) event_manager: RefCell<Option<SurfaceEventManager>>,
}

impl Window {
  pub(crate) fn surface(&self) -> &Surface {
    &self.surface
  }

  pub fn wl_resource(&self) -> *mut wl_resource {
    self.surface.wl_resource()
  }

  pub fn wl_client(&self) -> *mut wl_client {
    unsafe {
      ffi_dispatch!(
        WAYLAND_SERVER_HANDLE,
        wl_resource_get_client,
        self.wl_resource()
      )
    }
  }

  pub fn wlr_surface(&self) -> *mut wlr_surface {
    self.surface.wlr_surface()
  }

  pub fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    self.surface.parent_wlr_surface()
  }

  pub fn on_entered_output(&self) -> &Event<Rc<Output>> {
    &self.on_entered_output
  }
  pub fn on_left_output(&self) -> &Event<Rc<Output>> {
    &self.on_left_output
  }
  pub fn on_destroy(&self) -> &EventOnce<()> {
    &self.on_destroy
  }

  fn position_displacement(&self) -> Displacement {
    let parent_displacement = self
      .surface
      .parent_wlr_surface()
      .and_then(|parent_wlr_surface| {
        self
          .window_manager
          .windows()
          .find(|w| w.wlr_surface() == parent_wlr_surface)
      })
      .map(|w| w.buffer_extents().top_left().as_displacement())
      .unwrap_or_default();

    self.top_left.borrow().as_displacement()
      + parent_displacement
      + self.surface.parent_displacement()
      - self.surface.buffer_displacement()
  }

  /// The position and size of the window
  pub fn extents(&self) -> Rectangle {
    self.surface.extents() + self.position_displacement()
  }

  /// The position and size of the buffer
  ///
  /// When a client draws client-side shadows (like GTK)
  /// this is larger than the window extents to also fit
  /// said shadows.
  pub fn buffer_extents(&self) -> Rectangle {
    let surface = unsafe { &*self.wlr_surface() };

    let buffer_rect = Rectangle {
      top_left: Point {
        x: surface.current.dx,
        y: surface.current.dy,
      },
      size: Size {
        width: surface.current.width,
        height: surface.current.height,
      },
    };

    buffer_rect + self.position_displacement()
  }

  /// Atomically updates position and size
  ///
  /// As size updates have to be communicated to the client,
  /// this will not cause an immediately observable effect.
  pub fn set_extents(&self, extents: &Rectangle) {
    self.pending_updates.borrow_mut().insert(
      self.surface.resize(extents.size),
      PendingUpdate {
        top_left: extents.top_left(),
      },
    );
  }

  pub fn move_to(&self, top_left: Point) {
    *self.top_left.borrow_mut() = top_left;

    self.surface.move_to(top_left);
    self.update_outputs();
  }

  pub fn resize(&self, size: Size) {
    self.surface.resize(size);
  }

  pub fn min_height(&self) -> Option<u32> {
    self.surface.min_height()
  }
  pub fn max_height(&self) -> Option<u32> {
    self.surface.max_height()
  }
  pub fn min_width(&self) -> Option<u32> {
    self.surface.min_width()
  }
  pub fn max_width(&self) -> Option<u32> {
    self.surface.max_width()
  }

  pub fn activated(&self) -> bool {
    self.surface.activated()
  }
  pub fn can_receive_focus(&self) -> bool {
    self.surface.can_receive_focus()
  }
  pub fn set_activated(&self, activated: bool) {
    self.surface.set_activated(activated);
  }

  pub fn maximized(&self) -> bool {
    self.surface.maximized()
  }
  pub fn set_maximized(&self, maximized: bool) {
    self.surface.set_maximized(maximized);
  }
  pub fn fullscreen(&self) -> bool {
    self.surface.fullscreen()
  }
  pub fn set_fullscreen(&self, fullscreen: bool) {
    self.surface.set_fullscreen(fullscreen);
  }
  pub fn resizing(&self) -> bool {
    self.surface.resizing()
  }
  pub fn set_resizing(&self, resizing: bool) {
    self.surface.set_resizing(resizing);
  }

  pub fn app_id(&self) -> Option<String> {
    self.surface.app_id()
  }
  pub fn title(&self) -> Option<String> {
    self.surface.title()
  }

  /// Outputs that the window currently appears on.
  pub fn outputs(&self) -> Ref<Vec<Rc<Output>>> {
    self.outputs.borrow()
  }
  /// Positions on the screen where for example a dock shows an icon for
  /// the window, which can be used as a target for a minimize animation.
  pub fn minimize_targets(&self) -> Ref<Vec<MinimizeTarget>> {
    self.minimize_targets.borrow()
  }

  pub fn ask_client_to_close(&self) {
    self.surface.ask_client_to_close()
  }

  pub(crate) fn update_outputs(&self) {
    for output in self.output_manager.outputs().iter() {
      let previously_on_output = self.outputs().iter().any(|o| o == output);
      let currently_on_output = output.extents().overlaps(&self.extents());

      if currently_on_output && !previously_on_output {
        self.outputs.borrow_mut().push(output.clone());
        self.on_entered_output.fire(output.clone());
      } else if !currently_on_output && previously_on_output {
        self.outputs.borrow_mut().retain(|o| o != output);
        self.on_left_output.fire(output.clone());
      }
    }
  }
}

impl PartialEq for Window {
  fn eq(&self, other: &Window) -> bool {
    self.surface == other.surface
  }
}

pub(crate) struct WindowCommitEvent {
  pub(crate) serial: u32,
}

pub(crate) struct WindowResizeEvent {
  pub(crate) edges: u32,
}

pub(crate) struct WindowMaximizeEvent {
  pub(crate) maximize: bool,
}

pub(crate) struct WindowFullscreenEvent {
  pub(crate) fullscreen: bool,
  pub(crate) output: Option<*mut wlr_output>,
}

pub struct ForeignToplevelHandle(*mut wlr_foreign_toplevel_handle_v1);

impl ForeignToplevelHandle {
  pub fn raw_ptr(&self) -> *mut wlr_foreign_toplevel_handle_v1 {
    self.0
  }

  pub fn title(&self) -> Cow<str> {
    let title: &CStr = unsafe { CStr::from_ptr((*self.0).title) };
    title.to_string_lossy()
  }
  pub(crate) fn set_title<S: Into<Vec<u8>>>(&mut self, title: S) -> Result<(), NulError> {
    let title = CString::new(title)?;
    unsafe {
      wlr_foreign_toplevel_handle_v1_set_title(self.0, title.as_ptr());
    }
    Ok(())
  }

  pub fn app_id(&self) -> Cow<str> {
    let app_id: &CStr = unsafe { CStr::from_ptr((*self.0).app_id) };
    app_id.to_string_lossy()
  }

  pub(crate) fn set_app_id<S: Into<Vec<u8>>>(&mut self, app_id: S) -> Result<(), NulError> {
    let app_id = CString::new(app_id)?;
    unsafe {
      wlr_foreign_toplevel_handle_v1_set_app_id(self.0, app_id.as_ptr());
    }
    Ok(())
  }
}

pub(crate) struct ForeignToplevelEventHandler {
  pub(crate) handle: *mut wlr_foreign_toplevel_handle_v1,
  pub(crate) wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  pub(crate) output_manager: Rc<OutputManager>,
  pub(crate) window: Weak<Window>,
}

impl ForeignToplevelEventHandler {
  fn request_activate(&self, event: wlr_foreign_toplevel_handle_v1_activated_event) {
    if let Some(window) = self.window.upgrade() {
      let handle = ForeignToplevelHandle(event.toplevel);
      let request = ActivateRequest {
        window: window.clone(),
        originator: RequestOriginator::Foreign(&handle),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_activate(request);
    }
  }
  fn request_close(&self) {
    if let Some(window) = self.window.upgrade() {
      let handle = ForeignToplevelHandle(self.handle);
      let request = CloseRequest {
        window: window.clone(),
        originator: RequestOriginator::Foreign(&handle),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_close(request);
    }
  }
  fn request_fullscreen(&self, event: wlr_foreign_toplevel_handle_v1_fullscreen_event) {
    if let Some(window) = self.window.upgrade() {
      let handle = ForeignToplevelHandle(event.toplevel);
      let request = FullscreenRequest {
        window: window.clone(),
        fullscreen: event.fullscreen,
        output: self
          .output_manager
          .outputs()
          .iter()
          .find(|o| o.raw_ptr() == event.output)
          .cloned(),
        originator: RequestOriginator::Foreign(&handle),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_fullscreen(request);
    }
  }
  fn request_maximize(&self, event: wlr_foreign_toplevel_handle_v1_maximized_event) {
    if let Some(window) = self.window.upgrade() {
      let handle = ForeignToplevelHandle(event.toplevel);
      let request = MaximizeRequest {
        window: window.clone(),
        maximize: event.maximized,
        originator: RequestOriginator::Foreign(&handle),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_maximize(request);
    }
  }
  fn request_minimize(&self, event: wlr_foreign_toplevel_handle_v1_minimized_event) {
    if let Some(window) = self.window.upgrade() {
      let handle = ForeignToplevelHandle(event.toplevel);
      let request = MinimizeRequest {
        window: window.clone(),
        minimize: event.minimized,
        originator: RequestOriginator::Foreign(&handle),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_minimize(request);
    }
  }
  fn set_rectangle(&self, event: wlr_foreign_toplevel_handle_v1_set_rectangle_event) {
    if let Some(window) = self.window.upgrade() {
      if event.width > 0 || event.height > 0 {
        let rectangle = Rectangle {
          top_left: Point {
            x: event.x,
            y: event.y,
          },
          size: Size {
            width: event.width,
            height: event.height,
          },
        };
        let mut minimize_targets = window.minimize_targets.borrow_mut();
        let existing_target = minimize_targets
          .iter_mut()
          .find(|target| target.surface == event.surface);
        if let Some(existing_target) = existing_target {
          existing_target.rectangle = rectangle;
        } else {
          minimize_targets.push(MinimizeTarget {
            rectangle,
            surface: event.surface,
          });
        }
      } else {
        window
          .minimize_targets
          .borrow_mut()
          .retain(|target| target.surface != event.surface);
      }
    }
  }
}

wayland_listener!(
  pub(crate) ForeignToplevelEventManager,
  ForeignToplevelEventHandler,
  [
    request_activate => request_activate_func: |this: &mut ForeignToplevelEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_activate(*(data as *mut wlr_foreign_toplevel_handle_v1_activated_event))
    };
    request_close => request_close_func: |this: &mut ForeignToplevelEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_close()
    };
    request_fullscreen => request_fullscreen_func: |this: &mut ForeignToplevelEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_fullscreen(*(data as *mut wlr_foreign_toplevel_handle_v1_fullscreen_event));
    };
    request_maximize => request_maximize_func: |this: &mut ForeignToplevelEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_maximize(*(data as *mut wlr_foreign_toplevel_handle_v1_maximized_event));
    };
    request_minimize => request_minimize_func: |this: &mut ForeignToplevelEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_minimize(*(data as *mut wlr_foreign_toplevel_handle_v1_minimized_event));
    };
    set_rectangle => set_rectangle_func: |this: &mut ForeignToplevelEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.set_rectangle(*(data as *mut wlr_foreign_toplevel_handle_v1_set_rectangle_event));
    };
  ]
);

pub(crate) struct WindowEventHandler {
  pub(crate) wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  pub(crate) output_manager: Rc<OutputManager>,
  pub(crate) window_manager: Rc<WindowManager>,
  pub(crate) cursor_manager: Rc<CursorManager>,
  pub(crate) window: Weak<Window>,
  pub(crate) foreign_toplevel_handle: Option<ForeignToplevelHandle>,
  pub(crate) foreign_toplevel_event_manager: Option<Pin<Box<ForeignToplevelEventManager>>>,
}

impl WindowEventHandler {
  pub(crate) fn map(&mut self) {
    if let Some(window) = self.window.upgrade() {
      if window.surface().is_toplevel() {
        unsafe {
          let foreign_toplevel_handle = wlr_foreign_toplevel_handle_v1_create(
            self.window_manager.raw_foreign_toplevel_manager(),
          );
          let mut event_manager = ForeignToplevelEventManager::new(ForeignToplevelEventHandler {
            handle: foreign_toplevel_handle,
            wm_policy_manager: self.wm_policy_manager.clone(),
            output_manager: self.output_manager.clone(),
            window: self.window.clone(),
          });
          event_manager.request_activate(&mut (*foreign_toplevel_handle).events.request_activate);
          event_manager.request_close(&mut (*foreign_toplevel_handle).events.request_close);
          event_manager
            .request_fullscreen(&mut (*foreign_toplevel_handle).events.request_fullscreen);
          event_manager.request_maximize(&mut (*foreign_toplevel_handle).events.request_maximize);
          event_manager.request_minimize(&mut (*foreign_toplevel_handle).events.request_minimize);
          event_manager.set_rectangle(&mut (*foreign_toplevel_handle).events.set_rectangle);

          self
            .foreign_toplevel_handle
            .replace(ForeignToplevelHandle(foreign_toplevel_handle));
          self.foreign_toplevel_event_manager.replace(event_manager);

          window.on_entered_output.subscribe(Box::new(move |output| {
            wlr_foreign_toplevel_handle_v1_output_enter(foreign_toplevel_handle, output.raw_ptr());
          }));
          window.on_left_output.subscribe(Box::new(move |output| {
            wlr_foreign_toplevel_handle_v1_output_leave(foreign_toplevel_handle, output.raw_ptr());
          }));

          if let Some(app_id) = window.app_id() {
            let result = ForeignToplevelHandle(foreign_toplevel_handle).set_app_id(app_id);
            if result.is_err() {
              error!(
                "Error setting app_id of window to \"{}\"",
                window.app_id().unwrap_or_default()
              );
            }
          }
          if let Some(title) = window.title() {
            let result = ForeignToplevelHandle(foreign_toplevel_handle).set_title(title);
            if result.is_err() {
              error!(
                "Error setting title of window to \"{}\"",
                window.title().unwrap_or_default()
              );
            }
          }
        };
      }
      window.update_outputs();
      self
        .wm_policy_manager
        .borrow_mut()
        .handle_window_ready(window.clone());
      *window.mapped.borrow_mut() = true;
    }
  }

  pub(crate) fn unmap(&mut self) {
    if let Some(window) = self.window.upgrade() {
      *window.mapped.borrow_mut() = false;
      if let Some(handle) = self.foreign_toplevel_handle.take() {
        unsafe {
          wlr_foreign_toplevel_handle_v1_destroy(handle.0);
        }
      }
      self.foreign_toplevel_event_manager.take();
    }
  }

  pub(crate) fn destroy(&mut self) {
    debug!("WindowEventHandler::destroy");
    if let Some(window) = self.window.upgrade() {
      window.on_destroy.fire(());
      self
        .wm_policy_manager
        .borrow_mut()
        .advise_delete_window(window.clone());
      self.window_manager.destroy_window(window.clone());
    }
  }

  pub(crate) fn commit(&mut self, event: WindowCommitEvent) {
    if let Some(window) = self.window.upgrade() {
      if !window.can_receive_focus() && self.window_manager.window_has_focus(&window) {
        self.window_manager.blur();
      }

      match window.pending_updates.borrow_mut().remove(&event.serial) {
        Some(update) => {
          window.move_to(update.top_left);
        }
        _ => {
          window.update_outputs();
        }
      }
      self
        .wm_policy_manager
        .borrow_mut()
        .advise_configured_window(window.clone());
    }
  }

  pub(crate) fn request_move(&mut self) {
    if let Some(window) = self.window.upgrade() {
      let request = MoveRequest {
        window: window.clone(),
        drag_point: self.cursor_manager.position()
          - FPoint::from(window.extents().top_left()).as_displacement(),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_move(request);
    }
  }

  pub(crate) fn request_resize(&mut self, event: WindowResizeEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = ResizeRequest {
        window: window.clone(),
        cursor_position: self.cursor_manager.position(),
        edges: WindowEdge::from_bits_truncate(event.edges),
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_resize(request);
    }
  }

  pub(crate) fn request_maximize(&mut self, event: WindowMaximizeEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = MaximizeRequest {
        window: window.clone(),
        maximize: event.maximize,
        originator: RequestOriginator::Application,
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_maximize(request);
    }
  }
  pub(crate) fn request_fullscreen(&mut self, event: WindowFullscreenEvent) {
    if let Some(window) = self.window.upgrade() {
      let request = FullscreenRequest {
        window: window.clone(),
        fullscreen: event.fullscreen,
        output: self
          .output_manager
          .outputs()
          .iter()
          .find(|o| Some(o.raw_ptr()) == event.output)
          .cloned(),
        originator: RequestOriginator::Application,
      };

      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_fullscreen(request);
    }
  }
  pub(crate) fn request_minimize(&mut self) {
    if let Some(window) = self.window.upgrade() {
      self
        .wm_policy_manager
        .borrow_mut()
        .handle_request_minimize(MinimizeRequest {
          window: window.clone(),
          minimize: true,
          originator: RequestOriginator::Application,
        });
    }
  }

  pub(crate) fn updated_app_id(&mut self) {
    if let Some(window) = self.window.upgrade() {
      if let Some(ref mut handle) = self.foreign_toplevel_handle {
        if let Some(app_id) = window.app_id() {
          let result = handle.set_app_id(app_id);
          if result.is_err() {
            error!(
              "Error setting app_id of window to \"{}\"",
              window.app_id().unwrap_or_default()
            );
          }
        }
      }
    }
  }
  pub(crate) fn updated_title(&mut self) {
    if let Some(window) = self.window.upgrade() {
      if let Some(ref mut handle) = self.foreign_toplevel_handle {
        if let Some(title) = window.title() {
          let result = handle.set_title(title);
          if result.is_err() {
            error!(
              "Error setting title of window to \"{}\"",
              window.title().unwrap_or_default()
            );
          }
        }
      }
    }
  }
}
