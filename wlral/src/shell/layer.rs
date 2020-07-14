use crate::geometry::*;
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window::*;
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::{WindowLayer, WindowManager, WindowManagerExt};
use log::{debug, error, trace};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use wlroots_sys::*;

#[derive(Debug, PartialEq, Eq)]
pub struct LayerSurfaceState(*mut wlr_layer_surface_v1_state);

impl LayerSurfaceState {
  pub fn attached_edges(&self) -> WindowEdge {
    unsafe { WindowEdge::from_bits_truncate((*self.0).anchor) }
  }
  pub fn set_attached_edges(&self, attached_edges: WindowEdge) {
    unsafe { (*self.0).anchor = attached_edges.bits() }
  }

  pub fn layer(&self) -> Result<WindowLayer, ()> {
    unsafe {
      #[allow(non_upper_case_globals)]
      match (*self.0).layer {
        zwlr_layer_shell_v1_layer_ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND => {
          Ok(WindowLayer::Background)
        }
        zwlr_layer_shell_v1_layer_ZWLR_LAYER_SHELL_V1_LAYER_BOTTOM => Ok(WindowLayer::Bottom),
        zwlr_layer_shell_v1_layer_ZWLR_LAYER_SHELL_V1_LAYER_TOP => Ok(WindowLayer::Top),
        zwlr_layer_shell_v1_layer_ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY => Ok(WindowLayer::Overlay),
        _ => Err(()),
      }
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LayerSurface(*mut wlr_layer_surface_v1);

impl LayerSurface {
  pub(crate) fn from_wlr_surface(wlr_surface: *mut wlr_surface) -> Result<LayerSurface, ()> {
    unsafe {
      if wlr_surface_is_layer_surface(wlr_surface) {
        let layer_surface = wlr_layer_surface_v1_from_wlr_surface(wlr_surface);
        Ok(LayerSurface(layer_surface))
      } else {
        Err(())
      }
    }
  }

  pub fn current(&self) -> LayerSurfaceState {
    unsafe { LayerSurfaceState(&mut (*self.0).current) }
  }
  pub fn client_pending(&self) -> LayerSurfaceState {
    unsafe { LayerSurfaceState(&mut (*self.0).client_pending) }
  }
  pub fn server_pending(&self) -> LayerSurfaceState {
    unsafe { LayerSurfaceState(&mut (*self.0).server_pending) }
  }
}

impl SurfaceExt for LayerSurface {
  fn wl_resource(&self) -> *mut wl_resource {
    unsafe { (*self.0).resource }
  }

  fn wlr_surface(&self) -> *mut wlr_surface {
    unsafe { (*self.0).surface }
  }

  fn parent_wlr_surface(&self) -> Option<*mut wlr_surface> {
    None
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
    Displacement::ZERO
  }

  fn extents(&self) -> Rectangle {
    unsafe {
      Rectangle {
        top_left: Point::ZERO,
        size: Size {
          width: (*self.0).current.desired_width as i32,
          height: (*self.0).current.desired_height as i32,
        },
      }
    }
  }

  fn move_to(&self, _top_left: Point) {}

  fn resize(&self, size: Size) -> u32 {
    unsafe {
      wlr_layer_surface_v1_configure(self.0, size.width as u32, size.height as u32);
    }
    0
  }

  fn can_receive_focus(&self) -> bool {
    unsafe { (*self.current().0).keyboard_interactive }
  }
  fn activated(&self) -> bool {
    false
  }
  fn set_activated(&self, _activated: bool) -> u32 {
    0
  }

  fn maximized(&self) -> bool {
    false
  }
  fn set_maximized(&self, _maximized: bool) -> u32 {
    0
  }
  fn fullscreen(&self) -> bool {
    false
  }
  fn set_fullscreen(&self, _fullscreen: bool) -> u32 {
    0
  }
  fn resizing(&self) -> bool {
    false
  }
  fn set_resizing(&self, _resizing: bool) -> u32 {
    0
  }

  fn app_id(&self) -> Option<String> {
    None
  }
  fn title(&self) -> Option<String> {
    None
  }

  fn ask_client_to_close(&self) {
    unsafe {
      wlr_layer_surface_v1_close(self.0);
    }
  }
}

wayland_listener!(
  pub LayerSurfaceEventManager,
  WindowEventHandler,
  [
    map => map_func: |this: &mut LayerSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.map()
    };
    unmap => unmap_func: |this: &mut LayerSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.unmap()
    };
    destroy => destroy_func: |this: &mut LayerSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.destroy();
    };
    commit => commit_func: |this: &mut LayerSurfaceEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      if let Some(window) = handler.window.upgrade() {
        update_anchor_edges(handler.output_manager.clone(), &window);
        if let Surface::Layer(ref layer_surface_v1) = window.surface {
          handler.commit(WindowCommitEvent {
            serial: (*layer_surface_v1.0).configure_serial,
          });
        }
      }
    };
  ]
);

pub struct LayersEventHandler {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  output_manager: Rc<dyn OutputManager>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<CursorManager>,
}
impl LayersEventHandler {
  fn new_surface(&mut self, layer_surface: *mut wlr_layer_surface_v1) {
    debug!("LayersEventHandler::new_surface");

    // Assign an output if the client did not request one
    unsafe {
      if (*layer_surface).output.is_null() {
        // TODO: Actually find the active output
        match self.output_manager.outputs().borrow().first() {
          Some(active_output) => {
            trace!(
              "LayersEventHandler::new_surface: Surface did not specify an output, picked: {0}",
              active_output.description()
            );
            (*layer_surface).output = active_output.output;
          }
          None => {
            debug!("LayersEventHandler::new_surface: Closing surface as there are no outputs");
            wlr_layer_surface_v1_close(layer_surface);
            return;
          }
        }
      } else {
        let output = self
          .output_manager
          .outputs()
          .borrow()
          .clone()
          .into_iter()
          .find(|output| output.raw_ptr() == (*layer_surface).output);

        if let Some(output) = output {
          trace!(
            "LayersEventHandler::new_surface: Surface did specify output: {0}",
            output.description()
          );
        } else {
          debug!(
            "LayersEventHandler::new_surface: Closing surface as it requested an invalid output"
          );
          wlr_layer_surface_v1_close(layer_surface);
          return;
        }
      }
    }

    let surface = LayerSurface(layer_surface);
    let layer = match surface.client_pending().layer() {
      Ok(layer) => layer,
      Result::Err(_) => {
        debug!("LayersEventHandler::new_surface: Closing surface as it requested an invalid layer");
        unsafe {
          wlr_layer_surface_v1_close(layer_surface);
        }
        return;
      }
    };

    let window = self
      .window_manager
      .new_window(layer, Surface::Layer(surface));

    let mut event_manager = LayerSurfaceEventManager::new(WindowEventHandler {
      wm_policy_manager: self.wm_policy_manager.clone(),
      output_manager: self.output_manager.clone(),
      window_manager: self.window_manager.clone(),
      cursor_manager: self.cursor_manager.clone(),
      window: Rc::downgrade(&window),
    });

    unsafe {
      event_manager.map(&mut (*layer_surface).events.map);
      event_manager.unmap(&mut (*layer_surface).events.unmap);
      event_manager.destroy(&mut (*layer_surface).events.destroy);
      event_manager.commit(&mut (*(*layer_surface).surface).events.commit);
    }

    *window.event_manager.borrow_mut() = Some(SurfaceEventManager::Layer(event_manager));

    update_anchor_edges(self.output_manager.clone(), &window);

    let output_manager = &self.output_manager;
    let subscription_id = self.output_manager.on_output_layout_change().subscribe(
      listener!(output_manager, window => move || {
        update_anchor_edges(output_manager.clone(), &window);
      }),
    );
    window
      .on_destroy()
      .then(listener!(output_manager => move || {
        output_manager.on_output_layout_change().unsubscribe(subscription_id);
      }));

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_new_window(window);
  }
}

fn update_anchor_edges(output_manager: Rc<dyn OutputManager>, window: &Window) {
  if let Surface::Layer(surface) = window.surface() {
    let attached_edges = surface.client_pending().attached_edges();
    let margins = unsafe { (*surface.client_pending().0).margin };

    let configured = unsafe { (*surface.0).configured };
    let output = output_manager
      .outputs()
      .borrow()
      .clone()
      .into_iter()
      .find(|output| output.raw_ptr() == unsafe { (*surface.0).output });
    let output = match output {
      Some(output) => output,
      None => {
        error!("LayerShell::update_anchor_edges: Could not find output for layer surface");
        unsafe {
          wlr_layer_surface_v1_close(surface.0);
        }
        return;
      }
    };

    let mut extents = window.extents();
    if attached_edges.contains(WindowEdge::TOP) && attached_edges.contains(WindowEdge::BOTTOM) {
      extents.size.height = output.size().height() - (margins.top + margins.bottom) as i32;
    }
    if attached_edges.contains(WindowEdge::LEFT) && attached_edges.contains(WindowEdge::RIGHT) {
      extents.size.width = output.size().width() - (margins.left + margins.right) as i32;
    }
    if attached_edges.contains(WindowEdge::TOP) {
      extents.top_left.y = output.top_left().y() + margins.top as i32;
    } else if attached_edges.contains(WindowEdge::BOTTOM) {
      extents.top_left.y = output.top_left().y() + output.size().height()
        - extents.size.height
        - margins.bottom as i32;
    } else {
      extents.top_left.y =
        output.top_left().y() + output.size().height() / 2 - extents.size.height / 2;
    }
    if attached_edges.contains(WindowEdge::LEFT) {
      extents.top_left.x = output.top_left().x() + margins.left as i32;
    } else if attached_edges.contains(WindowEdge::RIGHT) {
      extents.top_left.x =
        output.top_left().x() + output.size().width() - extents.size.width - margins.right as i32;
    } else {
      extents.top_left.x =
        output.top_left().x() + output.size().width() / 2 - extents.size.width / 2;
    }
    if !configured || extents.size != window.extents().size {
      unsafe {
        wlr_layer_surface_v1_configure(
          surface.0,
          extents.size.width as u32,
          extents.size.height as u32,
        )
      };
    }
    window.move_to(extents.top_left);
  }
}

wayland_listener!(
  pub LayersEventManager,
  Rc<RefCell<LayersEventHandler>>,
  [
     new_surface => new_surface_func: |this: &mut LayersEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().new_surface(data as _)
     };
  ]
);

#[allow(unused)]
pub(crate) struct LayerShellManager {
  layer_shell: *mut wlr_layer_shell_v1,

  event_manager: Pin<Box<LayersEventManager>>,
  event_handler: Rc<RefCell<LayersEventHandler>>,
}

impl LayerShellManager {
  pub(crate) fn init(
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    output_manager: Rc<dyn OutputManager>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<CursorManager>,
    display: *mut wl_display,
  ) -> LayerShellManager {
    debug!("LayerShellManager::init");

    let layer_shell = unsafe { wlr_layer_shell_v1_create(display) };

    let event_handler = Rc::new(RefCell::new(LayersEventHandler {
      wm_policy_manager,
      output_manager,
      window_manager,
      cursor_manager,
    }));

    let mut event_manager = LayersEventManager::new(event_handler.clone());
    unsafe {
      event_manager.new_surface(&mut (*layer_shell).events.new_surface);
    }

    LayerShellManager {
      layer_shell,

      event_manager,
      event_handler,
    }
  }
}
