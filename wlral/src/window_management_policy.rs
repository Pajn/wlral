use crate::geometry::FPoint;
use crate::output::Output;
use crate::window::{ForeignToplevelHandle, Window, WindowEdge};
use std::cell::RefCell;
use std::{fmt::Debug, rc::Rc};

pub enum RequestOriginator<'a> {
  Application,
  Foreign(&'a ForeignToplevelHandle),
}

pub struct ActivateRequest<'a> {
  pub window: Rc<Window>,
  /// Always Foreign
  pub originator: RequestOriginator<'a>,
}

pub struct CloseRequest<'a> {
  pub window: Rc<Window>,
  /// Always Foreign
  pub originator: RequestOriginator<'a>,
}

/// Request from the client to initiate a move of the window, most
/// commonly from mouse down on a CSD
pub struct MoveRequest {
  pub window: Rc<Window>,
  /// Window local coordinates of where on the window the drag was initiated
  pub drag_point: FPoint,
}

/// Request from the client to initiate a resize of the window, most
/// commonly from mouse down on a CSD
pub struct ResizeRequest {
  pub window: Rc<Window>,
  /// Global coordinates of the cursor position where the resize was initiated
  pub cursor_position: FPoint,
  pub edges: WindowEdge,
}

pub struct MaximizeRequest<'a> {
  pub window: Rc<Window>,
  pub maximize: bool,
  pub originator: RequestOriginator<'a>,
}

pub struct FullscreenRequest<'a> {
  pub window: Rc<Window>,
  pub fullscreen: bool,
  pub output: Option<Rc<Output>>,
  pub originator: RequestOriginator<'a>,
}

pub struct MinimizeRequest<'a> {
  pub window: Rc<Window>,
  pub minimize: bool,
  pub originator: RequestOriginator<'a>,
}

pub trait WindowManagementPolicy {
  fn handle_window_ready(&mut self, _window: Rc<Window>) {}
  fn advise_new_window(&mut self, _window: Rc<Window>) {}
  fn advise_configured_window(&mut self, _window: Rc<Window>) {}
  fn advise_delete_window(&mut self, _window: Rc<Window>) {}

  fn handle_request_activate(&mut self, _request: ActivateRequest) {}
  fn handle_request_close(&mut self, _request: CloseRequest) {}
  fn handle_request_move(&mut self, _request: MoveRequest) {}
  fn handle_request_resize(&mut self, _request: ResizeRequest) {}
  fn handle_request_maximize(&mut self, _request: MaximizeRequest) {}
  fn handle_request_fullscreen(&mut self, _request: FullscreenRequest) {}
  fn handle_request_minimize(&mut self, _request: MinimizeRequest) {}

  fn advise_output_create(&mut self, _output: Rc<Output>) {}
  fn advise_output_update(&mut self, _output: Rc<Output>) {}
  fn advise_output_delete(&mut self, _output: Rc<Output>) {}
}

pub(crate) struct WmPolicyManager {
  policy: Option<Rc<RefCell<dyn WindowManagementPolicy>>>,
}

impl Debug for WmPolicyManager {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "WmPolicyManager")
  }
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

  fn handle_request_activate(&mut self, request: ActivateRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_activate(request)
    }
  }
  fn handle_request_close(&mut self, request: CloseRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_close(request)
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
  fn handle_request_minimize(&mut self, request: MinimizeRequest) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_minimize(request)
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
