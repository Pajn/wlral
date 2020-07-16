use crate::{event::Event, output_manager::OutputManager, wayland_timer::WlTimer};
use log::{debug, error};
use std::{cell::RefCell, pin::Pin, rc::Rc};
use wlroots_sys::*;

struct OutputTest {
  old_config: *mut wlr_output_configuration_v1,
  new_config: *mut wlr_output_configuration_v1,
  // Stored here for ownership so that the timer is cleaned up when the test is
  #[allow(unused)]
  timer: WlTimer,
}

impl Drop for OutputTest {
  fn drop(&mut self) {
    unsafe {
      if !self.new_config.is_null() {
        wlr_output_configuration_v1_destroy(self.new_config);
      }
      if !self.old_config.is_null() {
        wlr_output_configuration_v1_destroy(self.old_config);
      }
    }
  }
}

// wlr-output-management-unstable-v1
/// Implements the wlr-output-management protocol.
///  This protocol allows clients to configure
/// and test size, position, scale, etc. of connected outputs. */
pub struct OutputManagementProtocol {
  is_applying_output_config: RefCell<bool>,
  pending_output_test: RefCell<Option<OutputTest>>,
  pending_test_timeout_ms: RefCell<u32>,

  on_output_management_test_started: Event<()>,
  on_output_management_test_timed_out: Event<()>,

  output_manager: Rc<OutputManager>,
  output_manager_v1: *mut wlr_output_manager_v1,
  event_manager: RefCell<Option<Pin<Box<OututManagementProtocolEventManager>>>>,
}

impl OutputManagementProtocol {
  pub(crate) fn init(
    output_manager: Rc<OutputManager>,
    pending_test_timeout_ms: u32,
  ) -> Rc<OutputManagementProtocol> {
    let output_manager_v1 = unsafe { wlr_output_manager_v1_create(output_manager.raw_display()) };

    let output_management = Rc::new(OutputManagementProtocol {
      is_applying_output_config: RefCell::new(false),
      pending_output_test: RefCell::new(None),
      pending_test_timeout_ms: RefCell::new(pending_test_timeout_ms),

      on_output_management_test_started: Event::default(),
      on_output_management_test_timed_out: Event::default(),

      output_manager: output_manager.clone(),
      output_manager_v1,
      event_manager: RefCell::new(None),
    });

    output_manager
      .on_output_layout_change()
      .subscribe(listener!(output_management => move || {
        // Multiple change events will be sent while applying an output config.
        // Don't bother sending an updated configuration in that case,
        // the configuration will be sent by output_config_apply().
        if !*output_management.is_applying_output_config.borrow() {
          // Create a new configuration object and send it to all connected
          // clients.
          unsafe {
            if let Some(config) = output_management.create_output_config() {
              // wlr_output_configuration_v1_destroy() doesn't need to be called here.
              // Setting the configuration calls destroy for us.
              wlr_output_manager_v1_set_configuration(output_manager_v1, config);
            }
          }
        }
      }));

    let mut event_manager = OututManagementProtocolEventManager::new(output_management.clone());

    unsafe {
      event_manager.apply(&mut (*output_manager_v1).events.apply);
      event_manager.test(&mut (*output_manager_v1).events.test);
    }

    *output_management.event_manager.borrow_mut() = Some(event_manager);

    output_management
  }

  /// Takes the current configuration of all outputs and turns it into a
  /// wlr_output_configuration_v1 object suitable for sending to clients.
  unsafe fn create_output_config(&self) -> Option<*mut wlr_output_configuration_v1> {
    let config = wlr_output_configuration_v1_create();
    if config.is_null() {
      return None;
    }

    for output in self.output_manager.outputs().iter() {
      let head = wlr_output_configuration_head_v1_create(config, output.raw_ptr());
      if head.is_null() {
        wlr_output_configuration_v1_destroy(config);
        return None;
      }

      let output_layout = self.output_manager.raw_output_layout();
      let output_box = wlr_output_layout_get_box(output_layout, output.raw_ptr());
      if !output_box.is_null() {
        (*head).state.x = (*output_box).x;
        (*head).state.y = (*output_box).y;
      }
    }

    Some(config)
  }

  /// Takes an output configuration object and commits its settings to all
  /// active outputs.
  unsafe fn apply_output_config(&self, config: *mut wlr_output_configuration_v1) {
    debug!("OutputManagementProtocol::apply_output_config");
    // wlr_output_commit() is being called in a loop, and it can trigger
    // an output_layout.change event each time it's called.
    *self.is_applying_output_config.borrow_mut() = true;

    wl_list_for_each!(
      (*config).heads,
      link,
      (head: wlr_output_configuration_head_v1) => {
        let output = (*head).state.output;
        let output_layout = self.output_manager.raw_output_layout();
        if (*head).state.enabled && !(*output).enabled {
          wlr_output_layout_add_auto(output_layout, output);
        } else if !(*head).state.enabled && (*output).enabled {
          wlr_output_layout_remove(output_layout, output);
        }
        wlr_output_enable(output, (*head).state.enabled);
        // All other settings only have an effect if the output is enabled.
        if (*head).state.enabled {
          if !(*head).state.mode.is_null() {
            wlr_output_set_mode(output, (*head).state.mode);
          } else {
            wlr_output_set_custom_mode(output,
                (*head).state.custom_mode.width, (*head).state.custom_mode.height,
                (*head).state.custom_mode.refresh);
          }
          wlr_output_layout_move(output_layout, output,
              (*head).state.x, (*head).state.y);
          wlr_output_set_scale(output, (*head).state.scale as f32);
          wlr_output_set_transform(output, (*head).state.transform);
        }
        wlr_output_commit(output);
      }
    );

    *self.is_applying_output_config.borrow_mut() = false;
  }

  pub fn raw_output_manager(&self) -> *mut wlr_output_manager_v1 {
    self.output_manager_v1
  }

  pub fn pending_test_timeout_ms(&self) -> u32 {
    *self.pending_test_timeout_ms.borrow()
  }

  pub fn set_pending_test_timeout_ms(&self, timeout: u32) {
    *self.pending_test_timeout_ms.borrow_mut() = timeout
  }

  pub fn has_pending_test(&self) -> bool {
    self.pending_output_test.borrow().is_some()
  }

  pub fn apply_pending_test(&self) -> Result<(), ()> {
    debug!("OutputManagementProtocol::apply_pending_test");
    if let Some(test) = self.pending_output_test.borrow_mut().take() {
      unsafe {
        wlr_output_configuration_v1_send_succeeded(test.new_config);
      }
      Ok(())
    } else {
      Err(())
    }
  }

  /// Change the output configuration back to the old one and tell the
  /// client the new one failed
  pub fn cancel_pending_test(&self) -> Result<(), ()> {
    debug!("OutputManagementProtocol::cancel_pending_test");
    if let Some(test) = self.pending_output_test.borrow_mut().take() {
      unsafe {
        self.apply_output_config(test.old_config);
        wlr_output_configuration_v1_send_failed(test.new_config);
      }
      Ok(())
    } else {
      Err(())
    }
  }
}

trait OutputManagementProtocolExt {
  unsafe fn test_output_config(&self, config: *mut wlr_output_configuration_v1) -> Result<(), ()>;
}

impl OutputManagementProtocolExt for Rc<OutputManagementProtocol> {
  unsafe fn test_output_config(&self, config: *mut wlr_output_configuration_v1) -> Result<(), ()> {
    debug!("OutputManagementProtocol::test_output_config: Testing new output config");
    // We can not handle multiple simultaneous tests.
    if self.pending_output_test.borrow().is_some() {
      error!("OutputManagementProtocol::test_output_config: Previous test already active");
      return Err(());
    }

    let output_manager_protocol = self.clone();
    let timer = WlTimer::init(
      self.output_manager.raw_display(),
      *self.pending_test_timeout_ms.borrow(),
      move || {
        debug!("OutputManagementProtocol::test_output_config: Timeout reached, reverting config");
        if output_manager_protocol.cancel_pending_test().is_err() {
          error!("OutputManagementProtocol::test_output_config: Error when canceling test after a timeout");
        }
        output_manager_protocol
          .on_output_management_test_timed_out
          .fire(());
      },
    )?;
    let current_config = match self.create_output_config() {
      Some(config) => config,
      None => return Err(()),
    };
    let test = OutputTest {
      new_config: config,
      old_config: current_config,
      timer,
    };

    self.pending_output_test.borrow_mut().replace(test);
    // Apply the new configuration so the user can see the result.
    self.apply_output_config(config);

    self.on_output_management_test_started.fire(());

    Ok(())
  }
}

wayland_listener!(
  OututManagementProtocolEventManager,
  Rc<OutputManagementProtocol>,
  [
    apply => apply_func: |this: &mut OututManagementProtocolEventManager, data: *mut libc::c_void,| unsafe {
      // This event is raised by a client requesting a permanent change
      // to the output configuration.
      let handler = &this.data;
      let config = data as *mut _;
      handler.apply_output_config(config);
      wlr_output_configuration_v1_send_succeeded(config);
      wlr_output_configuration_v1_destroy(config);
    };
    test => test_func: |this: &mut OututManagementProtocolEventManager, data: *mut libc::c_void,| unsafe {
      // This event is raised by a client requesting a test for a new
      // output configuration.
      let handler = &this.data;
      let config = data as *mut _;

      if handler.test_output_config(config).is_err() {
        wlr_output_configuration_v1_send_failed(config);
        wlr_output_configuration_v1_destroy(config);
      }
    };
  ]
);
