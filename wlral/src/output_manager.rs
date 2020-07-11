use crate::output::{Output, OutputEvents};
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::WindowManager;
use log::{debug, error};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::{ffi::CStr, time::Instant};
use wayland_sys::server::wl_display;
use wlroots_sys::*;

#[cfg(test)]
use mockall::*;

wayland_listener!(
  pub OutputManagerEventManager,
  Rc<OutputManagerImpl>,
  [
    new_output => new_output_func: |this: &mut OutputManagerEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut manager = this.data;
      let wm_policy_manager = manager.wm_policy_manager.clone();
      let window_manager = manager.window_manager.clone();
      let output_manager = manager.clone();
      let renderer = manager.renderer;
      let output_layout = manager.output_layout;
      manager.new_output(
        Output {
          wm_policy_manager,
          window_manager,
          output_manager,
          renderer,
          output_layout,
          output: data as *mut wlr_output,
          created_at: Instant::now(),
          event_manager: RefCell::new(None),
        }
      );
    };
  ]
);

pub trait OutputEventListener {
  fn new_output(&self, output: &Output);
  fn destroyed_output(&self, output: &Output);
}
#[cfg_attr(test, automock)]
pub trait OutputManager {
  fn outputs(&self) -> &RefCell<Vec<Rc<Output>>>;
  fn subscribe(&self, listener: Rc<dyn OutputEventListener>);
}

pub struct OutputManagerImpl {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  renderer: *mut wlr_renderer,
  output_layout: *mut wlr_output_layout,
  #[allow(unused)]
  xdg_output_manager_v1: *mut wlr_xdg_output_manager_v1,
  outputs: RefCell<Vec<Rc<Output>>>,
  event_listeners: RefCell<Vec<Rc<dyn OutputEventListener>>>,

  event_manager: RefCell<Option<Pin<Box<OutputManagerEventManager>>>>,
}

impl OutputManager for OutputManagerImpl {
  fn outputs(&self) -> &RefCell<Vec<Rc<Output>>> {
    &self.outputs
  }

  fn subscribe(&self, listener: Rc<dyn OutputEventListener>) {
    self.event_listeners.borrow_mut().push(listener);
  }
}

impl OutputManagerImpl {
  pub(crate) fn init(
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    display: *mut wl_display,
    backend: *mut wlr_backend,
    renderer: *mut wlr_renderer,
    output_layout: *mut wlr_output_layout,
  ) -> Rc<OutputManagerImpl> {
    let xdg_output_manager_v1 = unsafe { wlr_xdg_output_manager_v1_create(display, output_layout) };

    let output_manager = Rc::new(OutputManagerImpl {
      wm_policy_manager,
      window_manager,
      renderer,
      output_layout,
      xdg_output_manager_v1,
      outputs: RefCell::new(vec![]),
      event_listeners: RefCell::new(vec![]),

      event_manager: RefCell::new(None),
    });

    debug!("OutputManager::init");

    let mut event_manager = OutputManagerEventManager::new(output_manager.clone());

    unsafe {
      event_manager.new_output(&mut (*backend).events.new_output);
    }

    *output_manager.event_manager.borrow_mut() = Some(event_manager);

    output_manager
  }

  fn new_output(&self, output: Output) {
    let description: &CStr = unsafe { CStr::from_ptr((*output.raw_ptr()).description) };

    debug!(
      "OutputManager::new_output: {0}",
      description.to_str().unwrap_or("[description missing]")
    );

    if output.use_preferred_mode().is_err() {
      error!("Failed setting mode for new output");
      unsafe {
        wlr_output_destroy(output.raw_ptr());
      }
      return;
    }

    unsafe {
      // Adds this to the output layout. The add_auto function arranges outputs
      // from left-to-right in the order they appear. A more sophisticated
      // compositor would let the user configure the arrangement of outputs in the
      // layout.
      wlr_output_layout_add_auto(self.output_layout, output.raw_ptr());

      // Creating the global adds a wl_output global to the display, which Wayland
      // clients can see to find out information about the output (such as
      // DPI, scale factor, manufacturer, etc).
      wlr_output_create_global(output.raw_ptr());
    }

    let output = Rc::new(output);

    output.bind_events();

    self.outputs.borrow_mut().push(output.clone());

    for listener in self.event_listeners.borrow().iter() {
      listener.new_output(&output);
    }
    self
      .wm_policy_manager
      .borrow_mut()
      .advise_output_create(output);
  }

  pub(crate) fn destroy_output(&self, destroyed_output: Rc<Output>) {
    for listener in self.event_listeners.borrow().iter() {
      listener.destroyed_output(&destroyed_output);
    }

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_output_delete(destroyed_output.clone());

    self
      .outputs
      .borrow_mut()
      .retain(|output| output.raw_ptr() != destroyed_output.raw_ptr());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::output::OutputEventHandler;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let wm_policy_manager = Rc::new(RefCell::new(WmPolicyManager::new()));
    let window_manager = Rc::new(RefCell::new(WindowManager::init(ptr::null_mut())));
    let output_manager = Rc::new(OutputManagerImpl {
      wm_policy_manager: wm_policy_manager.clone(),
      window_manager: window_manager.clone(),
      renderer: ptr::null_mut(),
      output_layout: ptr::null_mut(),
      xdg_output_manager_v1: ptr::null_mut(),
      outputs: RefCell::new(vec![]),
      event_listeners: RefCell::new(vec![]),

      event_manager: RefCell::new(None),
    });
    let output = Rc::new(Output {
      wm_policy_manager,
      window_manager,
      output_manager: output_manager.clone(),
      renderer: ptr::null_mut(),
      output_layout: ptr::null_mut(),
      output: ptr::null_mut(),
      created_at: Instant::now(),
      event_manager: RefCell::new(None),
    });

    output_manager.outputs.borrow_mut().push(output.clone());

    let weak_output = Rc::downgrade(&output);
    drop(output);

    weak_output.upgrade().unwrap().destroy();

    assert!(output_manager.outputs.borrow().len() == 0);
    assert!(weak_output.upgrade().is_none());
  }
}
