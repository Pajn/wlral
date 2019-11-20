use crate::output::{Output, OutputEventManager};
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::window_manager::WindowManager;
use log::debug;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Instant;
use wlroots_sys::*;

#[cfg(test)]
use mockall::*;

wayland_listener!(
  pub OutputManagerEventManager,
  Rc<RefCell<OutputManagerImpl>>,
  [
    new_output => new_output_func: |this: &mut OutputManagerEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut manager = this.data;
      let wm_policy_manager = manager.borrow().wm_policy_manager.clone();
      let window_manager = manager.borrow().window_manager.clone();
      let output_manager = manager.clone();
      let renderer = manager.borrow().renderer;
      let output_layout = manager.borrow().output_layout;
      manager.borrow_mut().new_output(
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

#[cfg_attr(test, automock)]
pub trait OutputManager {
  fn outputs(&self) -> &Vec<Rc<Output>>;
}

pub struct OutputManagerImpl {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  renderer: *mut wlr_renderer,
  output_layout: *mut wlr_output_layout,
  outputs: Vec<Rc<Output>>,

  event_manager: Option<Pin<Box<OutputManagerEventManager>>>,
}

impl OutputManager for OutputManagerImpl {
  fn outputs(&self) -> &Vec<Rc<Output>> {
    &self.outputs
  }
}

impl OutputManagerImpl {
  pub(crate) fn init(
    wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
    window_manager: Rc<RefCell<WindowManager>>,
    backend: *mut wlr_backend,
    renderer: *mut wlr_renderer,
    output_layout: *mut wlr_output_layout,
  ) -> Rc<RefCell<OutputManagerImpl>> {
    let output_manager = Rc::new(RefCell::new(OutputManagerImpl {
      wm_policy_manager,
      window_manager,
      renderer,
      output_layout,
      outputs: vec![],

      event_manager: None,
    }));

    debug!("OutputManager::init");

    let mut event_manager = OutputManagerEventManager::new(output_manager.clone());

    unsafe {
      event_manager.new_output(&mut (*backend).events.new_output);
    }

    output_manager.borrow_mut().event_manager = Some(event_manager);

    output_manager
  }

  fn new_output(&mut self, output: Output) {
    debug!("new_output");

    output.use_preferred_mode();

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

    let mut event_manager = OutputEventManager::new(Rc::downgrade(&output));

    unsafe {
      event_manager.frame(&mut (*output.raw_ptr()).events.frame);
      event_manager.destroy(&mut (*output.raw_ptr()).events.destroy);
    }

    *output.event_manager.borrow_mut() = Some(event_manager);

    self.outputs.push(output.clone());

    self
      .wm_policy_manager
      .borrow_mut()
      .advise_output_create(output);
  }

  pub(crate) fn destroy_output(&mut self, destroyed_output: &Output) {
    self
      .outputs
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
    let output_manager = Rc::new(RefCell::new(OutputManagerImpl {
      wm_policy_manager: wm_policy_manager.clone(),
      window_manager: window_manager.clone(),
      renderer: ptr::null_mut(),
      output_layout: ptr::null_mut(),
      outputs: vec![],

      event_manager: None,
    }));
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

    output_manager.borrow_mut().outputs.push(output.clone());

    let weak_output = Rc::downgrade(&output);
    drop(output);

    weak_output.upgrade().unwrap().destroy();

    assert!(output_manager.borrow().outputs.len() == 0);
    assert!(weak_output.upgrade().is_none());
  }
}
