use crate::geometry::FPoint;
use crate::output::Output;
use crate::surface::Surface;
use bitflags::bitflags;
use std::cell::RefCell;
use std::rc::Rc;

bitflags! {
  pub struct WindowEdge: u32 {
    const NONE   = 0b0000;
    const TOP    = 0b0001;
    const BOTTOM = 0b0010;
    const LEFT   = 0b0100;
    const RIGHT  = 0b1000;
  }
}

pub struct MoveEvent {
  pub surface: Rc<Surface>,
  // Surface local coordinates of where on the window the drag was initiated
  pub drag_point: FPoint,
}

pub struct ResizeEvent {
  pub surface: Rc<Surface>,
  // Global coordinates of the cursor position where the resize was initiated
  pub cursor_position: FPoint,
  pub edges: WindowEdge,
}

pub trait WindowManagementPolicy {
  fn handle_window_ready(&mut self, _surface: Rc<Surface>) {}
  fn advise_new_window(&mut self, _surface: Rc<Surface>) {}
  fn advise_delete_window(&mut self, _surface: Rc<Surface>) {}

  /// request from client to initiate move
  fn handle_request_move(&mut self, _event: MoveEvent) {}
  /// request from client to initiate resize
  fn handle_request_resize(&mut self, _event: ResizeEvent) {}

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

  pub(crate) fn set_policy<T>(&mut self, policy: Rc<RefCell<T>>)
  where
    T: 'static + WindowManagementPolicy,
  {
    self.policy = Some(policy)
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

  fn handle_request_move(&mut self, event: MoveEvent) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_move(event)
    }
  }
  fn handle_request_resize(&mut self, event: ResizeEvent) {
    if let Some(ref mut policy) = self.policy {
      policy.borrow_mut().handle_request_resize(event)
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
