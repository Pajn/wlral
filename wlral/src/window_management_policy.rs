use crate::output::Output;
use crate::surface::Surface;
use std::cell::RefCell;
use std::rc::Rc;

pub trait WindowManagementPolicy {
  fn handle_window_ready(&mut self, _surface: Rc<Surface>) {}
  fn advise_new_window(&mut self, _surface: Rc<Surface>) {}
  fn advise_delete_window(&mut self, _surface: Rc<Surface>) {}

  fn advise_output_create(&mut self, _output: Rc<Output>) {}
  fn advise_output_delete(&mut self, _output: Rc<Output>) {}
}

pub(crate) struct WmManager {
  policy: Option<Rc<RefCell<dyn WindowManagementPolicy>>>,
}

impl WmManager {
  pub(crate) fn new() -> WmManager {
    WmManager { policy: None }
  }

  pub(crate) fn set_policy<T>(&mut self, policy: T)
  where
    T: 'static + WindowManagementPolicy,
  {
    self.policy = Some(Rc::new(RefCell::new(policy)))
  }
}

impl WindowManagementPolicy for WmManager {
  fn handle_window_ready(&mut self, surface: Rc<Surface>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_window_ready(surface)
    }
  }
  fn advise_new_window(&mut self, surface: Rc<Surface>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_new_window(surface)
    }
  }
  fn advise_delete_window(&mut self, surface: Rc<Surface>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_delete_window(surface)
    }
  }

  fn advise_output_create(&mut self, output: Rc<Output>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_output_create(output)
    }
  }
  fn advise_output_delete(&mut self, output: Rc<Output>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_output_delete(output)
    }
  }
}
