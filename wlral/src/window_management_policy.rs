use crate::geometry::FPoint;
use crate::output::Output;
use crate::window::{Window, WindowEdge};
use std::cell::RefCell;
use std::rc::Rc;

pub struct MoveRequest {
  pub window: Rc<Window>,
  // Window local coordinates of where on the window the drag was initiated
  pub drag_point: FPoint,
}

pub struct ResizeRequest {
  pub window: Rc<Window>,
  // Global coordinates of the cursor position where the resize was initiated
  pub cursor_position: FPoint,
  pub edges: WindowEdge,
}

pub struct MaximizeRequest {
  pub window: Rc<Window>,
  pub maximize: bool,
}

pub struct FullscreenRequest {
  pub window: Rc<Window>,
  pub fullscreen: bool,
  pub output: Option<Rc<Output>>,
}

pub trait WindowManagementPolicy {
  fn handle_window_ready(&mut self, _window: Rc<Window>) {}
  fn advise_new_window(&mut self, _window: Rc<Window>) {}
  fn advise_configured_window(&mut self, _window: Rc<Window>) {}
  fn advise_delete_window(&mut self, _window: Rc<Window>) {}

  /// request from client to initiate move
  fn handle_request_move(&mut self, _request: MoveRequest) {}
  /// request from client to initiate resize
  fn handle_request_resize(&mut self, _request: ResizeRequest) {}
  fn handle_request_maximize(&mut self, _request: MaximizeRequest) {}
  fn handle_request_fullscreen(&mut self, _request: FullscreenRequest) {}
  fn handle_request_minimize(&mut self, _window: Rc<Window>) {}

  fn advise_output_create(&mut self, _output: Rc<Output>) {}
  fn advise_output_update(&mut self, _output: Rc<Output>) {}
  fn advise_output_delete(&mut self, _output: Rc<Output>) {}
}

pub(crate) struct WmPolicyManager {
  policy: Option<Rc<RefCell<dyn WindowManagementPolicy>>>,
}

impl WmPolicyManager {
  pub(crate) fn new() -> WmPolicyManager {
    WmPolicyManager { policy: None }
  }

  pub(crate) fn set_policy<T>(&mut self, policy: Rc<RefCell<T>>)
  where
    T: 'static + WindowManagementPolicy,
  {
    self.policy = Some(policy)
  }
}

impl WindowManagementPolicy for WmPolicyManager {
  fn handle_window_ready(&mut self, window: Rc<Window>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_window_ready(window)
    }
  }
  fn advise_new_window(&mut self, window: Rc<Window>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_new_window(window)
    }
  }
  fn advise_configured_window(&mut self, window: Rc<Window>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_configured_window(window)
    }
  }
  fn advise_delete_window(&mut self, window: Rc<Window>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_delete_window(window)
    }
  }

  fn handle_request_move(&mut self, request: MoveRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_move(request)
    }
  }
  fn handle_request_resize(&mut self, request: ResizeRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_resize(request)
    }
  }
  fn handle_request_maximize(&mut self, request: MaximizeRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_maximize(request)
    }
  }
  fn handle_request_fullscreen(&mut self, request: FullscreenRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_fullscreen(request)
    }
  }
  fn handle_request_minimize(&mut self, window: Rc<Window>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_minimize(window)
    }
  }

  fn advise_output_create(&mut self, output: Rc<Output>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_output_create(output)
    }
  }
  fn advise_output_update(&mut self, output: Rc<Output>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_output_update(output)
    }
  }
  fn advise_output_delete(&mut self, output: Rc<Output>) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().advise_output_delete(output)
    }
  }
}
