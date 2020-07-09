use crate::geometry::*;
use crate::input::cursor::CursorManager;
use crate::output_manager::OutputManager;
use crate::surface::{Surface, SurfaceEventManager, SurfaceExt};
use crate::window::*;
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::{WindowManager, WindowManagerExt};
use log::debug;
use std::cell::RefCell;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wayland_sys::server::wl_display;
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
    false
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
        update_anchor_edges(&window);
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
  output_manager: Rc<RefCell<dyn OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  cursor_manager: Rc<RefCell<dyn CursorManager>>,
}
impl LayersEventHandler {
  fn new_surface(&mut self, layer_surface: *mut wlr_layer_surface_v1) {
    debug!("LayersEventHandler::new_surface");

    // Assign an output if the client did not request one
    unsafe {
      if (*layer_surface).output == ptr::null_mut() {
        // TODO: Actually find the active output
        match self.output_manager.borrow().outputs().first() {
          Some(active_output) => {
            (*layer_surface).output = active_output.output;
          }
          None => {
            wlr_layer_surface_v1_close(layer_surface);
            return;
          }
        }
      }
    }

    let surface = LayerSurface(layer_surface);

    let window = self.window_manager.new_window(Surface::Layer(surface));

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

    update_anchor_edges(&window);

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_new_window(window);
  }
}

fn update_anchor_edges(window: &Window) {
  if let Surface::Layer(surface) = window.surface() {
    let attached_edges = surface.client_pending().attached_edges();
    let margins = unsafe { (*surface.client_pending().0).margin };

    let configured = unsafe { (*surface.0).configured };
    let output_height = unsafe { (*(*surface.0).output).height };
    let output_width = unsafe { (*(*surface.0).output).width };

    let mut extents = window.extents();
    if attached_edges.contains(WindowEdge::TOP) && attached_edges.contains(WindowEdge::BOTTOM) {
      extents.size.height = output_height - (margins.top + margins.bottom) as i32;
    }
    if attached_edges.contains(WindowEdge::LEFT) && attached_edges.contains(WindowEdge::RIGHT) {
      extents.size.width = output_width - (margins.left + margins.right) as i32;
    }
    // TODO: Handle multiple monitors
    if attached_edges.contains(WindowEdge::TOP) {
      extents.top_left.y = margins.top as i32;
    } else if attached_edges.contains(WindowEdge::BOTTOM) {
      extents.top_left.y = output_height - extents.size.height - margins.bottom as i32;
    }
    if attached_edges.contains(WindowEdge::LEFT) {
      extents.top_left.x = margins.left as i32;
    } else if attached_edges.contains(WindowEdge::RIGHT) {
      extents.top_left.x = output_width - extents.size.width - margins.right as i32;
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
    output_manager: Rc<RefCell<dyn OutputManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    cursor_manager: Rc<RefCell<dyn CursorManager>>,
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
