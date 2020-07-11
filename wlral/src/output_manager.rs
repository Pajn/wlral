#[cfg_attr(test, allow(unused))]
use crate::output::{Output, OutputEvents};
use crate::window_management_policy::{WindowManagementPolicy, WmPolicyManager};
use crate::{
  event::{Event, EventOnce},
  window_manager::WindowManager,
};
#[cfg_attr(test, allow(unused))]
use log::{debug, error};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Instant;
use wayland_sys::server::wl_display;
use wlroots_sys::*;

#[cfg(test)]
use mockall::*;

wayland_listener!(
  pub OutputManagerEventManager,
  Rc<OutputManagerImpl>,
  [
    new_output => new_output_func: |this: &mut OutputManagerEventManager, data: *mut libc::c_void,| unsafe {
      new_output(this.data.clone(), data as *mut wlr_output)
    };
  ]
);

fn new_output(manager: Rc<OutputManagerImpl>, output: *mut wlr_output) {
  let wm_policy_manager = manager.wm_policy_manager.clone();
  let window_manager = manager.window_manager.clone();
  let renderer = manager.renderer;
  let output_layout = manager.output_layout;
  let output = Output {
    wm_policy_manager,
    window_manager,
    renderer,
    output_layout,
    output,
    created_at: Instant::now(),
    on_destroy: EventOnce::default(),
    event_manager: RefCell::new(None),
  };

  #[cfg(not(test))]
  {
    use std::ffi::CStr;
    let description: &CStr = unsafe { CStr::from_ptr((*output.raw_ptr()).description) };
    debug!(
      "OutputManager::new_output: {0}",
      description.to_str().unwrap_or("[description missing]")
    );
  }

  #[cfg(not(test))]
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
    wlr_output_layout_add_auto(manager.output_layout, output.raw_ptr());

    // Creating the global adds a wl_output global to the display, which Wayland
    // clients can see to find out information about the output (such as
    // DPI, scale factor, manufacturer, etc).
    wlr_output_create_global(output.raw_ptr());
  }

  let output = Rc::new(output);

  #[cfg(not(test))]
  output.bind_events();
  output
    .on_destroy
    .then(listener!(manager, output => move || {
      manager
        .wm_policy_manager
        .borrow_mut()
        .advise_output_delete(output.clone());

      manager
        .outputs
        .borrow_mut()
        .retain(|o| o.raw_ptr() != output.raw_ptr());
    }));

  manager.outputs.borrow_mut().push(output.clone());

  manager.on_new_output.fire(output.clone());

  manager
    .wm_policy_manager
    .borrow_mut()
    .advise_output_create(output);
}

#[cfg_attr(test, automock)]
pub trait OutputManager {
  fn outputs(&self) -> &RefCell<Vec<Rc<Output>>>;
  fn on_new_output(&self) -> &Event<Rc<Output>>;
}

pub struct OutputManagerImpl {
  wm_policy_manager: Rc<RefCell<WmPolicyManager>>,
  window_manager: Rc<RefCell<WindowManager>>,
  renderer: *mut wlr_renderer,
  output_layout: *mut wlr_output_layout,
  #[allow(unused)]
  xdg_output_manager_v1: *mut wlr_xdg_output_manager_v1,
  outputs: RefCell<Vec<Rc<Output>>>,
  on_new_output: Event<Rc<Output>>,

  event_manager: RefCell<Option<Pin<Box<OutputManagerEventManager>>>>,
}

impl OutputManager for OutputManagerImpl {
  fn outputs(&self) -> &RefCell<Vec<Rc<Output>>> {
    &self.outputs
  }

  fn on_new_output(&self) -> &Event<Rc<Output>> {
    &self.on_new_output
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
    debug!("OutputManager::init");

    let xdg_output_manager_v1 = unsafe { wlr_xdg_output_manager_v1_create(display, output_layout) };

    let output_manager = Rc::new(OutputManagerImpl {
      wm_policy_manager,
      window_manager,
      renderer,
      output_layout,
      xdg_output_manager_v1,
      outputs: RefCell::new(vec![]),
      on_new_output: Event::default(),

      event_manager: RefCell::new(None),
    });

    let mut event_manager = OutputManagerEventManager::new(output_manager.clone());

    unsafe {
      event_manager.new_output(&mut (*backend).events.new_output);
    }

    *output_manager.event_manager.borrow_mut() = Some(event_manager);

    output_manager
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::seat::SeatManager;
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let wm_policy_manager = Rc::new(RefCell::new(WmPolicyManager::new()));
    let seat_manager = SeatManager::mock(ptr::null_mut(), ptr::null_mut());
    let window_manager = Rc::new(RefCell::new(WindowManager::init(seat_manager)));
    let output_manager = Rc::new(OutputManagerImpl {
      wm_policy_manager: wm_policy_manager.clone(),
      window_manager: window_manager.clone(),
      renderer: ptr::null_mut(),
      output_layout: ptr::null_mut(),
      xdg_output_manager_v1: ptr::null_mut(),
      outputs: RefCell::new(vec![]),
      on_new_output: Event::default(),

      event_manager: RefCell::new(None),
    });

    new_output(output_manager.clone(), ptr::null_mut());
    let output = output_manager.outputs.borrow().first().unwrap().clone();

    let weak_output = Rc::downgrade(&output);
    drop(output);

    weak_output.upgrade().unwrap().on_destroy.fire(());

    assert!(output_manager.outputs.borrow().len() == 0);
    assert!(weak_output.upgrade().is_none());
  }
}
#[cfg(test)]
pub unsafe fn wlr_output_layout_add_auto(_: *mut wlr_output_layout, _: *mut wlr_output) {}
#[cfg(test)]
pub unsafe fn wlr_output_create_global(_: *mut wlr_output) {}
