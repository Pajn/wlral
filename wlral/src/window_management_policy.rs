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
  fn handle_window_ready(&self, _window: Rc<Window>) {}
  fn advise_new_window(&self, _window: Rc<Window>) {}
  fn advise_configured_window(&self, _window: Rc<Window>) {}
  fn advise_focused_window(&self, _window: Rc<Window>) {}
  fn advise_delete_window(&self, _window: Rc<Window>) {}

  fn handle_request_activate(&self, _request: ActivateRequest) {}
  fn handle_request_close(&self, _request: CloseRequest) {}
  fn handle_request_move(&self, _request: MoveRequest) {}
  fn handle_request_resize(&self, _request: ResizeRequest) {}
  fn handle_request_maximize(&self, _request: MaximizeRequest) {}
  fn handle_request_fullscreen(&self, _request: FullscreenRequest) {}
  fn handle_request_minimize(&self, _request: MinimizeRequest) {}

  fn advise_output_create(&self, _output: Rc<Output>) {}
  fn advise_output_update(&self, _output: Rc<Output>) {}
  fn advise_output_delete(&self, _output: Rc<Output>) {}
}

pub(crate) struct WmPolicyManager {
  policy: RefCell<Option<Rc<dyn WindowManagementPolicy>>>,
}

impl Debug for WmPolicyManager {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "WmPolicyManager")
  }
}

impl WmPolicyManager {
  pub(crate) fn new() -> WmPolicyManager {
    WmPolicyManager {
      policy: RefCell::new(None),
    }
  }

  pub(crate) fn set_policy<T>(&self, policy: Rc<T>)
  where
    T: 'static + WindowManagementPolicy,
  {
    self.policy.borrow_mut().replace(policy);
  }

  pub(crate) fn handle_window_ready(&self, window: Rc<Window>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_window_ready(window)
    }
  }
  pub(crate) fn advise_new_window(&self, window: Rc<Window>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_new_window(window)
    }
  }
  pub(crate) fn advise_configured_window(&self, window: Rc<Window>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_configured_window(window)
    }
  }
  pub(crate) fn advise_focused_window(&self, window: Rc<Window>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_focused_window(window)
    }
  }
  pub(crate) fn advise_delete_window(&self, window: Rc<Window>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_delete_window(window)
    }
  }

  pub(crate) fn handle_request_activate(&self, request: ActivateRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_activate(request)
    }
  }
  pub(crate) fn handle_request_close(&self, request: CloseRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_close(request)
    }
  }
  pub(crate) fn handle_request_move(&self, request: MoveRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_move(request)
    }
  }
  pub(crate) fn handle_request_resize(&self, request: ResizeRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_resize(request)
    }
  }
  pub(crate) fn handle_request_maximize(&self, request: MaximizeRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_maximize(request)
    }
  }
  pub(crate) fn handle_request_fullscreen(&self, request: FullscreenRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_fullscreen(request)
    }
  }
  pub(crate) fn handle_request_minimize(&self, request: MinimizeRequest) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.handle_request_minimize(request)
    }
  }

  pub(crate) fn advise_output_create(&self, output: Rc<Output>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_output_create(output)
    }
  }
  pub(crate) fn advise_output_update(&self, output: Rc<Output>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_output_update(output)
    }
  }
  pub(crate) fn advise_output_delete(&self, output: Rc<Output>) {
    if let Some(ref policy) = *self.policy.borrow() {
      policy.advise_output_delete(output)
    }
  }
}
